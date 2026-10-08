// Painel do alternador: placa de vidro fosco flutuante, do tamanho do conteúdo, com
// um cartão por aplicativo e as janelas em miniaturas quadradas ao vivo (DWM), com a
// imagem recortada para caber. Parar o mouse sobre uma miniatura abre a prévia da
// janela inteira. O anel da seleção e a sombra da prévia ficam numa camada própria
// por cima do painel: o DWM desenha as miniaturas por cima de tudo o que o painel
// pinta, e o anel ficava coberto pelas miniaturas vizinhas.
use super::janelas::Janela;
use crate::ffi::*;
use crate::formas::{smoothstep, v2, V2};
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

const CLASSE: &str = "FlowCursorAlternador";
const CLASSE_REALCE: &str = "FlowCursorAlternadorRealce";
/// Atraso antes de mostrar o painel: um Alt+Tab rápido troca de janela sem piscar nada na tela.
const ATRASO_MOSTRAR: Duration = Duration::from_millis(90);
/// Tempo parado sobre uma miniatura até abrir a prévia (só passar o mouse por cima não abre).
const ESPERA_PREVIA: Duration = Duration::from_millis(140);
/// Miniaturas por linha dentro de um cartão.
const COLUNAS_MAX: usize = 4;
/// Altura do cabeçalho de cada cartão (ícone e nome do aplicativo), em px a 100%.
const CABECALHO: f32 = 50.0;
/// Ajuste de tamanho (0 a 1) até onde o painel mostra uma lista em vez de miniaturas.
/// Fosco abaixo disso é vidro limpo (sem desfoque); a partir de VIDRO_ACRILICO, acrílico.
const VIDRO_LIMPO: f32 = 0.25;
const VIDRO_ACRILICO: f32 = 0.65;
const LIMITE_LISTA: f32 = 0.2;

// ---------- Geometria e molas ----------

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Ret {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Ret {
    fn contem(&self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
    fn centro(&self) -> V2 {
        v2(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    fn crescer(&self, d: f32) -> Ret {
        Ret { x: self.x - d, y: self.y - d, w: self.w + 2.0 * d, h: self.h + 2.0 * d }
    }
    fn escalar(&self, k: f32) -> Ret {
        let c = self.centro();
        Ret { x: c.x - self.w * k / 2.0, y: c.y - self.h * k / 2.0, w: self.w * k, h: self.h * k }
    }
    fn uniao(&self, o: &Ret) -> Ret {
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        Ret { x, y, w: (self.x + self.w).max(o.x + o.w) - x, h: (self.y + self.h).max(o.y + o.h) - y }
    }
    fn cortar(&self, o: &Ret) -> Option<Ret> {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        let w = (self.x + self.w).min(o.x + o.w) - x;
        let h = (self.y + self.h).min(o.y + o.h) - y;
        (w >= 1.0 && h >= 1.0).then_some(Ret { x, y, w, h })
    }
    /// Maior retângulo com a proporção `p` (largura/altura) que cabe dentro deste, centralizado.
    fn encaixar(&self, p: f32) -> Ret {
        if self.w / self.h > p {
            let w = self.h * p;
            Ret { x: self.x + (self.w - w) / 2.0, y: self.y, w, h: self.h }
        } else {
            let h = self.w / p;
            Ret { x: self.x, y: self.y + (self.h - h) / 2.0, w: self.w, h }
        }
    }
    /// Desloca para dentro de `limite` (sem mudar o tamanho, se couber).
    fn dentro_de(&self, limite: &Ret) -> Ret {
        let x = self.x.max(limite.x).min(limite.x + limite.w - self.w);
        let y = self.y.max(limite.y).min(limite.y + limite.h - self.h);
        Ret { x, y, ..*self }
    }
    fn inteiro(&self) -> RECT {
        RECT { left: self.x.round() as i32, top: self.y.round() as i32, right: (self.x + self.w).round() as i32, bottom: (self.y + self.h).round() as i32 }
    }
}

#[derive(Clone, Copy, Default)]
struct Mola {
    x: f32,
    v: f32,
    alvo: f32,
}

impl Mola {
    fn em(x: f32) -> Self {
        Mola { x, v: 0.0, alvo: x }
    }
    fn passo(&mut self, dt: f32, freq: f32, amort: f32) {
        let w = std::f32::consts::TAU * freq;
        let n = (dt / 0.002).ceil().max(1.0) as usize;
        let h = dt / n as f32;
        for _ in 0..n {
            let a = (self.alvo - self.x) * w * w - self.v * 2.0 * amort * w;
            self.v += a * h;
            self.x += self.v * h;
        }
    }
}

#[derive(Clone, Copy, Default)]
struct MolaRet([Mola; 4]);

impl MolaRet {
    fn em(r: Ret) -> Self {
        MolaRet([Mola::em(r.x), Mola::em(r.y), Mola::em(r.w), Mola::em(r.h)])
    }
    fn mirar(&mut self, r: Ret) {
        for (m, v) in self.0.iter_mut().zip([r.x, r.y, r.w, r.h]) {
            m.alvo = v;
        }
    }
    fn passo(&mut self, dt: f32, freq: f32, amort: f32) {
        for m in &mut self.0 {
            m.passo(dt, freq, amort);
        }
    }
    fn atual(&self) -> Ret {
        Ret { x: self.0[0].x, y: self.0[1].x, w: self.0[2].x, h: self.0[3].x }
    }
}

// ---------- Pixels (BGRA pré-multiplicado) ----------

fn desempacotar(c: u32) -> [f32; 4] {
    [((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0, (c >> 24) as f32 / 255.0]
}

fn empacotar(c: [f32; 4]) -> u32 {
    let a = c[3].clamp(0.0, 1.0);
    let q = |x: f32| (x.clamp(0.0, a) * 255.0 + 0.5) as u32;
    q(a) << 24 | q(c[0]) << 16 | q(c[1]) << 8 | q(c[2])
}

/// `cor` em RGBA comum; `cob` é a cobertura do pixel (0 a 1).
fn misturar(dst: &mut u32, cor: [f32; 4], cob: f32) {
    let a = cor[3] * cob;
    if a <= 0.0 {
        return;
    }
    let d = desempacotar(*dst);
    let k = 1.0 - a;
    *dst = empacotar([cor[0] * a + d[0] * k, cor[1] * a + d[1] * k, cor[2] * a + d[2] * k, a + d[3] * k]);
}

/// `src` pré-multiplicado por cima de `dst`.
fn sobrepor(dst: &mut u32, src: u32) {
    let a = (src >> 24) as f32 / 255.0;
    if a <= 0.0 {
        return;
    }
    if a >= 1.0 {
        *dst = src;
        return;
    }
    let s = desempacotar(src);
    let d = desempacotar(*dst);
    let k = 1.0 - a;
    *dst = empacotar([s[0] + d[0] * k, s[1] + d[1] * k, s[2] + d[2] * k, s[3] + d[3] * k]);
}

fn sd_caixa(p: V2, meia: V2, r: f32) -> f32 {
    let q = v2(p.x.abs() - meia.x + r, p.y.abs() - meia.y + r);
    v2(q.x.max(0.0), q.y.max(0.0)).len() + q.x.max(q.y).min(0.0) - r
}

/// Cor ARGB com a opacidade multiplicada por `a`.
fn com_alfa(cor: u32, a: f32) -> u32 {
    (((cor >> 24) as f32 * a.clamp(0.0, 1.0)) as u32) << 24 | (cor & 0xFF_FFFF)
}

struct Tela<'a> {
    px: &'a mut [u32],
    w: usize,
    h: usize,
    /// Só pinta dentro deste retângulo.
    recorte: Ret,
}

impl Tela<'_> {
    fn faixa(&self, r: Ret, margem: f32) -> (usize, usize, usize, usize) {
        let x0 = (r.x - margem).max(self.recorte.x).max(0.0).floor() as usize;
        let y0 = (r.y - margem).max(self.recorte.y).max(0.0).floor() as usize;
        let x1 = ((r.x + r.w + margem).min(self.recorte.x + self.recorte.w).ceil().max(0.0) as usize).min(self.w);
        let y1 = ((r.y + r.h + margem).min(self.recorte.y + self.recorte.h).ceil().max(0.0) as usize).min(self.h);
        (x0, y0, x1, y1)
    }

    /// Para cada pixel: cobertura = f(distância até a borda do retângulo arredondado, y relativo 0..1).
    fn forma(&mut self, r: Ret, raio: f32, margem: f32, cor: [f32; 4], f: impl Fn(f32, f32) -> f32) {
        let (x0, y0, x1, y1) = self.faixa(r, margem);
        let c = r.centro();
        let meia = v2(r.w / 2.0, r.h / 2.0);
        let raio = raio.min(meia.x).min(meia.y);
        for y in y0..y1 {
            let fy = ((y as f32 + 0.5 - r.y) / r.h.max(1.0)).clamp(0.0, 1.0);
            let linha = &mut self.px[y * self.w..(y + 1) * self.w];
            for (x, px) in linha.iter_mut().enumerate().take(x1).skip(x0) {
                let d = sd_caixa(v2(x as f32 + 0.5 - c.x, y as f32 + 0.5 - c.y), meia, raio);
                let cob = f(d, fy);
                if cob > 0.0 {
                    misturar(px, cor, cob);
                }
            }
        }
    }

    fn preencher(&mut self, r: Ret, raio: f32, cor: [f32; 4]) {
        self.forma(r, raio, 1.0, cor, |d, _| (0.5 - d).clamp(0.0, 1.0));
    }

    fn contornar(&mut self, r: Ret, raio: f32, largura: f32, cor: [f32; 4]) {
        self.forma(r, raio, largura + 1.0, cor, |d, _| (0.5 - (d.abs() - largura / 2.0)).clamp(0.0, 1.0));
    }

    /// Borda com luz vindo de cima: mais clara no topo, quase some embaixo (vidro).
    fn contornar_iluminado(&mut self, r: Ret, raio: f32, largura: f32, cor: [f32; 4], base: f32) {
        self.forma(r, raio, largura + 1.0, cor, move |d, fy| (0.5 - (d.abs() - largura / 2.0)).clamp(0.0, 1.0) * (base + (1.0 - base) * (1.0 - fy).powf(1.6)));
    }

    /// Brilho do vidro: degradê claro que desce do topo até `ate` (fração da altura).
    fn brilho(&mut self, r: Ret, raio: f32, cor: [f32; 4], ate: f32) {
        self.forma(r, raio, 1.0, cor, move |d, fy| (0.5 - d).clamp(0.0, 1.0) * (1.0 - smoothstep(0.0, ate, fy)));
    }

    /// Sombra só do lado de fora de `r`: esta camada fica por cima da miniatura e não pode escurecê-la.
    fn sombra_externa(&mut self, r: Ret, raio: f32, desfoque: f32, desce: f32, cor: [f32; 4]) {
        let sombra = Ret { y: r.y + desce, ..r };
        let (x0, y0, x1, y1) = self.faixa(sombra.uniao(&r), desfoque * 1.5);
        let meia = v2(r.w / 2.0, r.h / 2.0);
        let raio = raio.min(meia.x).min(meia.y);
        let (c, cs) = (r.centro(), sombra.centro());
        for y in y0..y1 {
            let linha = &mut self.px[y * self.w..(y + 1) * self.w];
            for (x, px) in linha.iter_mut().enumerate().take(x1).skip(x0) {
                let p = v2(x as f32 + 0.5, y as f32 + 0.5);
                let fora = (sd_caixa(v2(p.x - c.x, p.y - c.y), meia, raio) + 0.5).clamp(0.0, 1.0);
                if fora <= 0.0 {
                    continue;
                }
                let d = sd_caixa(v2(p.x - cs.x, p.y - cs.y), meia, raio);
                let cob = (1.0 - smoothstep(-desfoque, desfoque, d)) * fora;
                if cob > 0.0 {
                    misturar(px, cor, cob);
                }
            }
        }
    }

    /// Desenha um ícone do Windows (com transparência) em (x, y) no tamanho `lado`.
    /// O Windows reduz ícone grande pegando pixels soltos (fica serrilhado); aqui ele é
    /// desenhado em até 4x o tamanho e reduzido pela média, que deixa as bordas lisas.
    fn icone(&mut self, icone: HANDLE, x: f32, y: f32, lado: f32) {
        if icone == 0 {
            return;
        }
        let n = lado.round().max(1.0) as usize;
        let f = (256 / n).clamp(1, 4);
        let m = n * f;
        unsafe {
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER { biSize: 40, biWidth: m as i32, biHeight: -(m as i32), biPlanes: 1, biBitCount: 32, ..Default::default() },
                bmiColors: [0],
            };
            let mut bits: *mut c_void = null_mut();
            let dib = CreateDIBSection(0, &bmi, 0, &mut bits, 0, 0);
            if dib == 0 || bits.is_null() {
                return;
            }
            let dc = CreateCompatibleDC(0);
            let antigo = SelectObject(dc, dib);
            DrawIconEx(dc, 0, 0, icone, m as i32, m as i32, 0, 0, DI_NORMAL);
            let grande = std::slice::from_raw_parts(bits as *const u32, m * m);
            // Ícones antigos (sem canal alfa) saem com alfa zero: aí vale a cor como opaca.
            let tem_alfa = grande.iter().any(|&c| c >> 24 != 0);
            let (ox, oy) = (x.round() as i64, y.round() as i64);
            let area = (f * f) as u32;
            for j in 0..n {
                for i in 0..n {
                    let (dx, dy) = (ox + i as i64, oy + j as i64);
                    if dx < 0 || dy < 0 || dx as usize >= self.w || dy as usize >= self.h || !self.recorte.contem(dx as f32 + 0.5, dy as f32 + 0.5) {
                        continue;
                    }
                    // média dos f x f pixels (cores pré-multiplicadas, então a média é direta)
                    let mut soma = [0u32; 4];
                    for b in 0..f {
                        for a in 0..f {
                            let mut c = grande[(j * f + b) * m + i * f + a];
                            if !tem_alfa {
                                c = if c & 0xFF_FFFF != 0 { c | 0xFF00_0000 } else { 0 };
                            }
                            for (k, s) in soma.iter_mut().enumerate() {
                                *s += (c >> (k * 8)) & 255;
                            }
                        }
                    }
                    let c = (0..4).fold(0u32, |acc, k| acc | (((soma[k] + area / 2) / area) << (k * 8)));
                    sobrepor(&mut self.px[dy as usize * self.w + dx as usize], c);
                }
            }
            SelectObject(dc, antigo);
            DeleteDC(dc);
            DeleteObject(dib);
        }
    }
}

// ---------- Texto (GDI+) ----------

struct Texto {
    normal: *mut c_void,
    semi: *mut c_void,
    centro: *mut c_void,
    esquerda: *mut c_void,
    direita: *mut c_void,
    /// centralizado, quebrando em até duas linhas (títulos embaixo das miniaturas)
    centro_duas: *mut c_void,
}

/// Alinhamento centralizado em até duas linhas (para Rotulo::alinhar).
const CENTRO_DUAS_LINHAS: i32 = 100;

struct Rotulo {
    texto: String,
    r: Ret,
    tam: f32,
    semi: bool,
    cor: u32,
    alinhar: i32,
    /// cor da sombra de 1 px embaixo do texto (0 = sem sombra)
    sombra: u32,
}

impl Texto {
    fn novo() -> Option<Texto> {
        unsafe {
            let entrada = GdiplusStartupInput { GdiplusVersion: 1, DebugEventCallback: null(), SuppressBackgroundThread: 0, SuppressExternalCodecs: 0 };
            let mut token = 0usize;
            if GdiplusStartup(&mut token, &entrada, null_mut()) != 0 {
                return None;
            }
            let familia = |nomes: &[&str]| {
                for n in nomes {
                    let mut f: *mut c_void = null_mut();
                    if GdipCreateFontFamilyFromName(w(n).as_ptr(), null_mut(), &mut f) == 0 && !f.is_null() {
                        return f;
                    }
                }
                null_mut()
            };
            let normal = familia(&["Segoe UI Variable Text", "Segoe UI"]);
            let semi = familia(&["Segoe UI Variable Text Semibold", "Segoe UI Semibold", "Segoe UI"]);
            if normal.is_null() {
                return None;
            }
            let formato = |alinhamento: i32| {
                let mut f: *mut c_void = null_mut();
                GdipCreateStringFormat(STRING_FORMAT_NOWRAP, 0, &mut f);
                GdipSetStringFormatTrimming(f, STRING_TRIMMING_ELLIPSIS);
                GdipSetStringFormatAlign(f, alinhamento);
                GdipSetStringFormatLineAlign(f, STRING_ALIGN_CENTER);
                f
            };
            Some(Texto {
                normal,
                semi: if semi.is_null() { normal } else { semi },
                centro: formato(STRING_ALIGN_CENTER),
                esquerda: formato(STRING_ALIGN_NEAR),
                direita: formato(STRING_ALIGN_FAR),
                centro_duas: {
                    let mut f: *mut c_void = null_mut();
                    GdipCreateStringFormat(0, 0, &mut f);
                    GdipSetStringFormatTrimming(f, STRING_TRIMMING_ELLIPSIS);
                    GdipSetStringFormatAlign(f, STRING_ALIGN_CENTER);
                    GdipSetStringFormatLineAlign(f, STRING_ALIGN_NEAR);
                    f
                },
            })
        }
    }

    fn formato(&self, alinhar: i32) -> *mut c_void {
        match alinhar {
            STRING_ALIGN_CENTER => self.centro,
            STRING_ALIGN_FAR => self.direita,
            CENTRO_DUAS_LINHAS => self.centro_duas,
            _ => self.esquerda,
        }
    }

    fn pintar(&self, px: &mut [u32], w: usize, h: usize, rotulos: &[Rotulo]) {
        if rotulos.is_empty() {
            return;
        }
        unsafe {
            let mut bmp: *mut c_void = null_mut();
            if GdipCreateBitmapFromScan0(w as i32, h as i32, (w * 4) as i32, PIXEL_FORMAT_32BPP_PARGB, px.as_mut_ptr().cast(), &mut bmp) != 0 {
                return;
            }
            let mut g: *mut c_void = null_mut();
            GdipGetImageGraphicsContext(bmp, &mut g);
            GdipSetTextRenderingHint(g, TEXT_RENDERING_ANTIALIAS_GRIDFIT);
            for r in rotulos {
                let mut fonte: *mut c_void = null_mut();
                GdipCreateFont(if r.semi { self.semi } else { self.normal }, r.tam, FONT_REGULAR, UNIT_PIXEL, &mut fonte);
                let t = r.texto.encode_utf16().collect::<Vec<u16>>();
                if r.sombra != 0 {
                    let mut pincel: *mut c_void = null_mut();
                    GdipCreateSolidFill(r.sombra, &mut pincel);
                    let caixa = RectF { x: r.r.x, y: r.r.y + (r.tam / 14.0).max(1.0), w: r.r.w, h: r.r.h };
                    GdipDrawString(g, t.as_ptr(), t.len() as i32, fonte, &caixa, self.formato(r.alinhar), pincel);
                    GdipDeleteBrush(pincel);
                }
                let mut pincel: *mut c_void = null_mut();
                GdipCreateSolidFill(r.cor, &mut pincel);
                let caixa = RectF { x: r.r.x, y: r.r.y, w: r.r.w, h: r.r.h };
                GdipDrawString(g, t.as_ptr(), t.len() as i32, fonte, &caixa, self.formato(r.alinhar), pincel);
                GdipDeleteBrush(pincel);
                GdipDeleteFont(fonte);
            }
            GdipDeleteGraphics(g);
            GdipDisposeImage(bmp);
        }
    }

    /// Largura do texto em pixels.
    fn medir(&self, texto: &str, tam: f32, semi: bool) -> f32 {
        let estimativa = texto.chars().count() as f32 * tam * 0.55;
        unsafe {
            let mut px = [0u32; 4];
            let mut bmp: *mut c_void = null_mut();
            if GdipCreateBitmapFromScan0(2, 2, 8, PIXEL_FORMAT_32BPP_PARGB, px.as_mut_ptr().cast(), &mut bmp) != 0 {
                return estimativa;
            }
            let mut g: *mut c_void = null_mut();
            GdipGetImageGraphicsContext(bmp, &mut g);
            GdipSetTextRenderingHint(g, TEXT_RENDERING_ANTIALIAS_GRIDFIT);
            let mut fonte: *mut c_void = null_mut();
            GdipCreateFont(if semi { self.semi } else { self.normal }, tam, FONT_REGULAR, UNIT_PIXEL, &mut fonte);
            let t = texto.encode_utf16().collect::<Vec<u16>>();
            let caixa = RectF { x: 0.0, y: 0.0, w: 100_000.0, h: tam * 4.0 };
            let mut medida = RectF { x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
            let ok = GdipMeasureString(g, t.as_ptr(), t.len() as i32, fonte, &caixa, self.esquerda, &mut medida, null_mut(), null_mut()) == 0;
            GdipDeleteFont(fonte);
            GdipDeleteGraphics(g);
            GdipDisposeImage(bmp);
            if ok {
                medida.w
            } else {
                estimativa
            }
        }
    }
}

// ---------- Tema ----------

struct Tema {
    /// cor do desfoque do Windows (AABBGGRR); quanto menor o alfa, mais transparente
    acento: u32,
    /// tipo de vidro do Windows: limpo, desfoque ou acrílico
    estado_vidro: u32,
    /// véu de cor do vidro limpo (sem desfoque, o acento do Windows não entra)
    veu: [f32; 4],
    /// véu branco por cima do desfoque: dá o fosco
    tinta: [f32; 4],
    brilho: [f32; 4],
    borda: [f32; 4],
    cartao: [f32; 4],
    cartao_borda: [f32; 4],
    vaga: [f32; 4],
    etiqueta: [f32; 4],
    /// tom do ícone do app que desce do topo de cada cartão
    tinta_cabecalho: f32,
    /// sombra do nome do app (legibilidade no vidro)
    sombra_texto: u32,
    texto: u32,
    texto2: u32,
    texto3: u32,
    destaque: [f32; 4],
}

/// Os ajustes chegam de 0 a 1. `transparencia`: quanto do fundo aparece (o véu escuro
/// do vidro some; em 100% fica só a borda iluminada, como o Liquid Glass "Clear").
/// `fosco`: até VIDRO_LIMPO é vidro limpo, sem desfoque; depois desfoque e, a partir de
/// VIDRO_ACRILICO, acrílico com um véu branco que deixa o painel leitoso no fim.
/// `cor`: tom do ícone no topo dos cartões (0,5 é o padrão).
fn tema(escuro: bool, destaque: [f32; 3], transparencia: f32, fosco: f32, cor: f32) -> Tema {
    let d = [destaque[0], destaque[1], destaque[2], 1.0];
    let cor = cor * 2.0;
    let fosco = fosco.clamp(0.0, 1.0);
    let opaco = (1.0 - transparencia.clamp(0.0, 1.0)).powf(1.3);
    // O gradiente transparente do Windows (estado 2) sai opaco; o vidro limpo é a janela
    // sem acento nenhum, com o próprio canal alfa do painel por cima do que estiver atrás.
    let limpo = fosco < VIDRO_LIMPO;
    let estado_vidro = if limpo {
        ACCENT_DISABLED
    } else if fosco < VIDRO_ACRILICO {
        ACCENT_ENABLE_BLURBEHIND
    } else {
        ACCENT_ENABLE_ACRYLICBLURBEHIND
    };
    let acento = |base: u32, alfa: f32| (((alfa.clamp(0.0, 0.94) * 255.0) as u32) << 24) | (base & 0xFF_FFFF);
    // véu de cor desenhado pelo próprio painel: no vidro limpo faz o papel do acento
    let veu = |base: [f32; 3], alfa: f32| [base[0], base[1], base[2], if limpo { alfa.clamp(0.0, 0.94) } else { 0.0 }];
    // em 100% de transparência os cartões quase somem e sobra o contorno
    let leve = 0.35 + 0.65 * opaco;
    if escuro {
        Tema {
            acento: acento(0x1414_14, if limpo { 0.0 } else { opaco * 0.75 }),
            estado_vidro,
            veu: veu([0.08, 0.08, 0.08], 0.05 + opaco * 0.8),
            tinta: [1.0, 1.0, 1.0, 0.14 * fosco * fosco],
            brilho: [1.0, 1.0, 1.0, 0.075],
            borda: [1.0, 1.0, 1.0, 0.26 + 0.1 * (1.0 - opaco)],
            cartao: [1.0, 1.0, 1.0, 0.045 * leve.max(0.6)],
            cartao_borda: [1.0, 1.0, 1.0, 0.11 + 0.05 * (1.0 - opaco)],
            vaga: [0.0, 0.0, 0.0, 0.22 * leve],
            etiqueta: [0.07, 0.07, 0.09, 0.78],
            tinta_cabecalho: 0.13 * cor,
            sombra_texto: 0x8C00_0000,
            texto: 0xFFF5F5F7,
            texto2: 0xD9E6E6EB,
            texto3: 0x8CE6E6EB,
            destaque: d,
        }
    } else {
        Tema {
            acento: acento(0xF2F2_F2, if limpo { 0.0 } else { opaco * 2.9 }),
            estado_vidro,
            veu: veu([0.95, 0.95, 0.95], 0.06 + opaco * 2.0),
            tinta: [1.0, 1.0, 1.0, 0.88 * fosco * fosco],
            brilho: [1.0, 1.0, 1.0, 0.30 * leve],
            borda: [1.0, 1.0, 1.0, 0.85],
            cartao: [1.0, 1.0, 1.0, 0.34 * leve],
            cartao_borda: [1.0, 1.0, 1.0, 0.7],
            vaga: [0.0, 0.0, 0.0, 0.06],
            etiqueta: [0.98, 0.98, 0.99, 0.85],
            tinta_cabecalho: 0.16 * cor,
            sombra_texto: 0x99FF_FFFF,
            texto: 0xFF111114,
            texto2: 0xD92C2C30,
            texto3: 0x993C3C43,
            destaque: d,
        }
    }
}
fn tema_escuro() -> bool {
    unsafe {
        let mut chave: HANDLE = 0;
        if RegOpenKeyExW(HKEY_CURRENT_USER, w(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize").as_ptr(), 0, KEY_QUERY_VALUE, &mut chave) != 0 {
            return true;
        }
        let mut v = 1u32;
        let mut tam = 4u32;
        let ok = RegQueryValueExW(chave, w("AppsUseLightTheme").as_ptr(), null_mut(), null_mut(), (&mut v as *mut u32).cast(), &mut tam) == 0;
        RegCloseKey(chave);
        !ok || v == 0
    }
}

fn cor_destaque() -> [f32; 3] {
    unsafe {
        let mut c = 0u32;
        let mut opaca: BOOL = 0;
        if DwmGetColorizationColor(&mut c, &mut opaca) == 0 {
            let comp = |s: u32| ((c >> s) & 255) as f32 / 255.0;
            let (r, g, b) = (comp(16), comp(8), comp(0));
            // cor de destaque escura demais some no vidro: aí usa o azul
            if 0.3 * r + 0.59 * g + 0.11 * b > 0.18 {
                return [r, g, b];
            }
        }
        [0.04, 0.52, 1.0]
    }
}

fn hsv_para_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h6 = (h.rem_euclid(1.0)) * 6.0;
    let c = v * s;
    let x = c * (1.0 - ((h6 % 2.0) - 1.0).abs());
    let (r, g, b) = match h6 as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m]
}

/// Cor que representa o ícone do aplicativo: o matiz mais presente entre os pixels
/// coloridos, clareado para servir de tinta no vidro. `None` para ícone sem cor
/// (cinza, branco ou preto), que fica com o cartão neutro.
fn cor_do_icone(icone: HANDLE) -> Option<[f32; 3]> {
    if icone == 0 {
        return None;
    }
    let n = 32usize;
    let mut px = vec![0u32; n * n];
    Tela { px: &mut px, w: n, h: n, recorte: Ret { x: 0.0, y: 0.0, w: n as f32, h: n as f32 } }.icone(icone, 0.0, 0.0, n as f32);
    // 12 faixas de matiz: soma de matiz (como vetor, para o vermelho não se partir), saturação e peso
    let mut faixas = [[0.0f32; 4]; 12];
    let mut opacos = 0.0f32;
    for &c in &px {
        let [r, g, b, a] = desempacotar(c);
        if a < 0.5 {
            continue;
        }
        opacos += 1.0;
        let (r, g, b) = (r / a, g / a, b / a);
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let sat = if max > 0.0 { (max - min) / max } else { 0.0 };
        if sat < 0.25 || max < 0.2 {
            continue;
        }
        let d = max - min;
        let h = if max == r {
            ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        } / 6.0;
        let peso = a * sat * max;
        let f = &mut faixas[((h * 12.0) as usize).min(11)];
        let ang = h * std::f32::consts::TAU;
        f[0] += ang.cos() * peso;
        f[1] += ang.sin() * peso;
        f[2] += sat * peso;
        f[3] += peso;
    }
    let coloridos: f32 = faixas.iter().map(|f| f[3]).sum();
    if opacos < 1.0 || coloridos < 0.08 * opacos {
        return None;
    }
    let f = faixas.iter().max_by(|a, b| a[3].total_cmp(&b[3]))?;
    let h = f[1].atan2(f[0]) / std::f32::consts::TAU;
    let s = (f[2] / f[3]).clamp(0.4, 0.75);
    Some(hsv_para_rgb(h, s, 1.0))
}

/// Vidro: o desfoque com acrílico do Windows. O acrílico do Windows 11 (DWMSBT) vira cor
/// sólida em janela que não está ativa, e o painel nunca fica ativo para não tirar o
/// foco de ninguém; este desfoque não depende disso.
fn aplicar_vidro(hwnd: HWND, cor: u32, estado: u32) {
    let mut acento = ACCENT_POLICY { AccentState: estado, AccentFlags: 0, GradientColor: cor, AnimationId: 0 };
    let mut dados = WINDOWCOMPOSITIONATTRIBDATA { Attrib: WCA_ACCENT_POLICY, pvData: (&mut acento as *mut ACCENT_POLICY).cast(), cbData: std::mem::size_of::<ACCENT_POLICY>() };
    unsafe { SetWindowCompositionAttribute(hwnd, &mut dados) };
}

/// Tira acentos e caixa para a busca: "Configurações" casa com "configur".
pub fn normalizar(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            outro => outro,
        })
        .collect()
}

// ---------- Camada de destaque (anel e sombra, por cima das miniaturas) ----------

unsafe extern "system" fn procedimento_realce(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_NCHITTEST {
        return HTTRANSPARENT;
    }
    DefWindowProcW(h, msg, wp, lp)
}

/// Janela em camadas, transparente a cliques, que só cobre a área do anel e da sombra.
struct Realce {
    hwnd: HWND,
    dc: HANDLE,
    dib: HANDLE,
    dib_antigo: HANDLE,
    bits: *mut u32,
    cap: (usize, usize),
    px: Vec<u32>,
    visivel: bool,
    /// o que foi desenhado por último (para não refazer quadro igual)
    assinatura: Vec<i32>,
}

impl Realce {
    fn criar(dono: HWND) -> Option<Realce> {
        unsafe {
            let inst = GetModuleHandleW(null());
            let nome = w(CLASSE_REALCE);
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: 0,
                lpfnWndProc: Some(procedimento_realce),
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
            // com dono: fica sempre por cima do painel
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                nome.as_ptr(),
                w("Destaque do alternador").as_ptr(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                dono,
                0,
                inst,
                null(),
            );
            if hwnd == 0 {
                return None;
            }
            let sim: i32 = 1;
            DwmSetWindowAttribute(hwnd, DWMWA_TRANSITIONS_FORCEDISABLED, (&sim as *const i32).cast(), 4);
            Some(Realce { hwnd, dc: CreateCompatibleDC(0), dib: 0, dib_antigo: 0, bits: null_mut(), cap: (0, 0), px: Vec::new(), visivel: false, assinatura: Vec::new() })
        }
    }

    fn garantir(&mut self, w: usize, h: usize) -> bool {
        if w <= self.cap.0 && h <= self.cap.1 && !self.bits.is_null() {
            return true;
        }
        let nw = w.max(self.cap.0).div_ceil(128) * 128;
        let nh = h.max(self.cap.1).div_ceil(128) * 128;
        unsafe {
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER { biSize: 40, biWidth: nw as i32, biHeight: -(nh as i32), biPlanes: 1, biBitCount: 32, ..Default::default() },
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
            self.cap = (nw, nh);
        }
        true
    }

    /// Mostra os pixels de `self.px` (w x h) em (x, y) da tela.
    fn apresentar(&mut self, x: i32, y: i32, w: usize, h: usize) {
        if !self.garantir(w, h) {
            return;
        }
        unsafe {
            let destino = std::slice::from_raw_parts_mut(self.bits, self.cap.0 * self.cap.1);
            for j in 0..h {
                destino[j * self.cap.0..j * self.cap.0 + w].copy_from_slice(&self.px[j * w..(j + 1) * w]);
            }
            let mistura = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA };
            let pos = POINT { x, y };
            let tam = SIZE { cx: w as i32, cy: h as i32 };
            let origem = POINT { x: 0, y: 0 };
            UpdateLayeredWindow(self.hwnd, 0, &pos, &tam, self.dc, &origem, 0, &mistura, ULW_ALPHA);
            if !self.visivel {
                ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                self.visivel = true;
            }
        }
    }

    fn esconder(&mut self) {
        if self.visivel {
            unsafe { ShowWindow(self.hwnd, SW_HIDE) };
            self.visivel = false;
        }
        self.assinatura.clear();
    }
}

impl Drop for Realce {
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

// ---------- Painel ----------

pub struct Item {
    pub janela: Janela,
    /// lugar da miniatura quadrada na grade
    alvo: Ret,
    ret: MolaRet,
    /// 0 = quadrado com a imagem recortada; 1 = prévia da janela inteira, ampliada
    abre: Mola,
    opac: Mola,
    thumb: isize,
    /// tamanho da janela de origem, em pixels (0 se não deu para saber)
    fonte: (f32, f32),
    pub visivel: bool,
}

struct Grupo {
    app: String,
    icone: HANDLE,
    ret: Ret,
    n: usize,
}

pub enum Clique {
    Item,
    Nada,
}

pub struct Painel {
    hwnd: HWND,
    dc: HANDLE,
    dib: HANDLE,
    dib_antigo: HANDLE,
    bits: *mut u32,
    w: usize,
    h: usize,
    base: Vec<u32>,
    escala: f32,
    tema: Tema,
    texto: Option<Texto>,
    realce: Option<Realce>,
    /// cor de cada ícone de app (calcular é barato, mas a busca redesenha a cada tecla)
    cores: std::collections::HashMap<HANDLE, Option<[f32; 3]>>,
    pub itens: Vec<Item>,
    grupos: Vec<Grupo>,
    pub sel: usize,
    anel: MolaRet,
    hover: Option<usize>,
    hover_desde: Instant,
    /// miniatura com a prévia aberta (já trazida para cima das outras)
    trazido: Option<usize>,
    cursor_abertura: POINT,
    mouse_mexeu: bool,
    pub filtro: String,
    /// painel fixo na tela depois de soltar o Alt (aí a busca e o mouse ficam à vontade)
    fixo: bool,
    pub aberto: bool,
    mostrado: bool,
    aberto_em: Instant,
    t_ant: Instant,
    sujo_tudo: bool,
    busca: Ret,
    limite: Ret,
    monitor: Ret,
    pos: (f32, f32),
    lado_inicial: f32,
    /// lista (ícone e nome inteiro) em vez de miniaturas
    lista: bool,
    /// tamanho da lista, de 0 (compacta, como um menu) a 1 (a maior)
    lista_t: f32,
    /// largura da coluna com o nome dos apps na lista
    lista_app_w: f32,
    /// dos ajustes, de 0 a 1: tamanho das miniaturas e parte da tela que o painel pode ocupar
    pref_tamanho: f32,
    pref_painel: f32,
}

impl Painel {
    pub fn criar(procedimento: WNDPROC) -> Option<Painel> {
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
                hCursor: LoadCursorW(0, OCR_NORMAL as usize as *const u16),
                hbrBackground: 0,
                lpszMenuName: null(),
                lpszClassName: nome.as_ptr(),
                hIconSm: 0,
            };
            RegisterClassExW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                nome.as_ptr(),
                w("Alternador do FlowCursor").as_ptr(),
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
            // O conteúdo usa o canal alfa por cima do desfoque.
            let margens = MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 };
            DwmExtendFrameIntoClientArea(hwnd, &margens);
            let escuro = tema_escuro();
            let sim: i32 = 1;
            let redondo: i32 = DWMWCP_ROUND;
            DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, (&(escuro as i32) as *const i32).cast(), 4);
            DwmSetWindowAttribute(hwnd, DWMWA_TRANSITIONS_FORCEDISABLED, (&sim as *const i32).cast(), 4);
            DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, (&redondo as *const i32).cast(), 4);
            let c = crate::laco::config_atual();
            let tema = tema(escuro, cor_destaque(), c.alttab_transparencia / 100.0, c.alttab_fosco / 100.0, c.alttab_cor / 100.0);
            aplicar_vidro(hwnd, tema.acento, tema.estado_vidro);
            Some(Painel {
                hwnd,
                dc: CreateCompatibleDC(0),
                dib: 0,
                dib_antigo: 0,
                bits: null_mut(),
                w: 0,
                h: 0,
                base: Vec::new(),
                escala: 1.0,
                tema,
                texto: Texto::novo(),
                realce: Realce::criar(hwnd),
                cores: std::collections::HashMap::new(),
                itens: Vec::new(),
                grupos: Vec::new(),
                sel: 0,
                anel: MolaRet::default(),
                hover: None,
                hover_desde: Instant::now(),
                trazido: None,
                cursor_abertura: POINT::default(),
                mouse_mexeu: false,
                filtro: String::new(),
                fixo: false,
                aberto: false,
                mostrado: false,
                aberto_em: Instant::now(),
                t_ant: Instant::now(),
                sujo_tudo: true,
                busca: Ret::default(),
                limite: Ret::default(),
                monitor: Ret::default(),
                pos: (0.0, 0.0),
                lado_inicial: 0.0,
                lista: false,
                lista_t: 1.0,
                lista_app_w: 120.0,
                pref_tamanho: 0.6,
                pref_painel: 0.9,
            })
        }
    }

    pub fn mostrado(&self) -> bool {
        self.mostrado
    }

    pub fn mouse_mexeu(&self) -> bool {
        self.mouse_mexeu
    }

    /// Retângulo do painel na tela (para saber se um clique foi fora dele).
    pub fn na_tela(&self) -> RECT {
        RECT { left: self.pos.0 as i32, top: self.pos.1 as i32, right: self.pos.0 as i32 + self.w as i32, bottom: self.pos.1 as i32 + self.h as i32 }
    }

    pub fn fixar(&mut self) {
        if !self.fixo {
            self.fixo = true;
            self.renderizar_base();
            self.sujo_tudo = true;
        }
    }

    fn garantir_dib(&mut self, w: usize, h: usize) -> bool {
        if w == self.w && h == self.h && !self.bits.is_null() {
            return true;
        }
        unsafe {
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER { biSize: 40, biWidth: w as i32, biHeight: -(h as i32), biPlanes: 1, biBitCount: 32, ..Default::default() },
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
        }
        self.w = w;
        self.h = h;
        self.base = vec![0; w * h];
        true
    }

    /// Prepara o painel no monitor onde está o ponteiro (ainda sem mostrar).
    pub fn abrir(&mut self, janelas: Vec<Janela>, inicial: usize) -> bool {
        let mut pt = POINT::default();
        unsafe { GetCursorPos(&mut pt) };
        let mon = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) };
        let Some((r, _)) = crate::sistema::info_monitor(mon) else {
            crate::reg!("Alt+Tab: não achei o monitor");
            return false;
        };
        self.monitor = Ret { x: r.left as f32, y: r.top as f32, w: (r.right - r.left) as f32, h: (r.bottom - r.top) as f32 };
        self.escala = crate::sistema::escala_dpi(pt);
        let escuro = tema_escuro();
        let c = crate::laco::config_atual();
        self.tema = tema(escuro, cor_destaque(), c.alttab_transparencia / 100.0, c.alttab_fosco / 100.0, c.alttab_cor / 100.0);
        self.pref_tamanho = c.alttab_tamanho / 100.0;
        self.pref_painel = c.alttab_painel / 100.0;
        unsafe { DwmSetWindowAttribute(self.hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, (&(escuro as i32) as *const i32).cast(), 4) };
        aplicar_vidro(self.hwnd, self.tema.acento, self.tema.estado_vidro);
        self.itens = janelas
            .into_iter()
            .map(|j| Item {
                janela: j,
                alvo: Ret::default(),
                ret: MolaRet::default(),
                abre: Mola::em(0.0),
                opac: Mola::em(0.0),
                thumb: 0,
                fonte: (0.0, 0.0),
                visivel: true,
            })
            .collect();
        for item in &mut self.itens {
            if item.janela.minimizada {
                continue;
            }
            let mut t = 0isize;
            unsafe {
                if DwmRegisterThumbnail(self.hwnd, item.janela.hwnd, &mut t) == 0 {
                    item.thumb = t;
                    let mut tam = SIZE::default();
                    if DwmQueryThumbnailSourceSize(t, &mut tam) == 0 && tam.cx > 0 && tam.cy > 0 {
                        item.fonte = (tam.cx as f32, tam.cy as f32);
                    }
                }
            }
        }
        self.sel = inicial.min(self.itens.len().saturating_sub(1));
        self.filtro.clear();
        self.fixo = false;
        self.hover = None;
        self.trazido = None;
        unsafe { GetCursorPos(&mut self.cursor_abertura) };
        self.mouse_mexeu = false;
        if !self.organizar(true) {
            return false;
        }
        // abertura: cada miniatura nasce um pouco menor e transparente
        for item in &mut self.itens {
            item.ret = MolaRet::em(item.alvo.escalar(0.92));
            item.ret.mirar(item.alvo);
            item.opac = Mola { x: 0.0, v: 0.0, alvo: 1.0 };
        }
        if let Some(i) = self.itens.get(self.sel) {
            self.anel = MolaRet::em(i.alvo);
        }
        self.aberto = true;
        self.mostrado = false;
        self.aberto_em = Instant::now();
        self.t_ant = Instant::now();
        self.sujo_tudo = true;
        true
    }

    fn mostrar(&mut self) {
        self.renderizar_base();
        self.sujo_tudo = true;
        self.mostrado = true;
        self.t_ant = Instant::now();
        unsafe {
            ShowWindow(self.hwnd, SW_SHOWNA);
            SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER);
        }
        self.quadro();
    }

    pub fn fechar(&mut self) {
        if let Some(r) = self.realce.as_mut() {
            r.esconder();
        }
        unsafe {
            ShowWindow(self.hwnd, SW_HIDE);
            for item in &self.itens {
                if item.thumb != 0 {
                    DwmUnregisterThumbnail(item.thumb);
                }
            }
        }
        self.itens.clear();
        self.grupos.clear();
        self.aberto = false;
        self.mostrado = false;
        self.fixo = false;
        self.hover = None;
        self.trazido = None;
        self.filtro.clear();
    }

    pub fn selecionada(&self) -> Option<HWND> {
        self.itens.get(self.sel).filter(|i| i.visivel).map(|i| i.janela.hwnd)
    }

    /// Distribui as janelas: um cartão por aplicativo (o usado por último primeiro), com
    /// as miniaturas quadradas numa grade, do maior tamanho que deixa tudo caber. Ao abrir
    /// (`dimensionar`), o painel ganha o tamanho do conteúdo e fica no centro do monitor;
    /// na busca o tamanho fica fixo e as janelas se reorganizam dentro dele.
    fn organizar(&mut self, dimensionar: bool) -> bool {
        let s = self.escala;
        let (margem, topo, rodape) = (22.0 * s, 70.0 * s, 22.0 * s);
        let (pad, cab, folga, folga_g, titulo) = (14.0 * s, CABECALHO * s, 12.0 * s, 14.0 * s, 40.0 * s);
        let (largura_max, altura_max) = if dimensionar {
            (self.monitor.w * self.pref_painel - 2.0 * margem, self.monitor.h * self.pref_painel * 0.95 - topo - rodape)
        } else {
            (self.w as f32 - 2.0 * margem, self.h as f32 - topo - rodape)
        };

        // grupos na ordem de uso, só com as janelas que passam no filtro
        let mut ordem: Vec<(String, Vec<usize>)> = Vec::new();
        for (i, item) in self.itens.iter().enumerate() {
            if !item.visivel {
                continue;
            }
            match ordem.iter_mut().find(|(exe, _)| *exe == item.janela.exe) {
                Some((_, v)) => v.push(i),
                None => ordem.push((item.janela.exe.clone(), vec![i])),
            }
        }

        // Posiciona tudo para um lado de miniatura, com as fileiras de cartões centralizadas
        // na largura `w_ref` a partir de (ox, oy). Devolve (altura total, fileira mais larga).
        let posicionar = |lado: f32, w_ref: f32, ox: f32, oy: f32, itens: &mut Vec<Item>, grupos: &mut Vec<Grupo>| -> (f32, f32) {
            grupos.clear();
            let cabem = (((largura_max - 2.0 * pad + folga) / (lado + folga)).floor() as usize).max(1);
            let cartoes: Vec<(f32, f32, usize)> = ordem
                .iter()
                .map(|(_, idx)| {
                    let cols = idx.len().min(cabem).min(COLUNAS_MAX);
                    let linhas = idx.len().div_ceil(cols) as f32;
                    let cw = 2.0 * pad + cols as f32 * lado + (cols - 1) as f32 * folga;
                    let ch = cab + linhas * (lado + titulo) + (linhas - 1.0) * folga + pad * 0.5;
                    (cw, ch, cols)
                })
                .collect();
            let mut fileiras: Vec<(Vec<usize>, f32, f32)> = Vec::new();
            for (k, (cw, ch, _)) in cartoes.iter().enumerate() {
                let cabe = fileiras.last().is_some_and(|(_, lw, _)| lw + folga_g + cw <= largura_max);
                if cabe {
                    let f = fileiras.last_mut().unwrap();
                    f.0.push(k);
                    f.1 += folga_g + cw;
                    f.2 = f.2.max(*ch);
                } else {
                    fileiras.push((vec![k], *cw, *ch));
                }
            }
            let total: f32 = fileiras.iter().map(|f| f.2).sum::<f32>() + folga_g * (fileiras.len().max(1) - 1) as f32;
            let mais_larga = fileiras.iter().map(|f| f.1).fold(0.0, f32::max);
            let mut y = oy;
            for (ks, lw, lh) in &fileiras {
                let mut x = ox + (w_ref - lw) / 2.0;
                for &k in ks {
                    let (cw, ch, cols) = cartoes[k];
                    let idx = &ordem[k].1;
                    let primeiro = &itens[idx[0]].janela;
                    grupos.push(Grupo { app: primeiro.app.clone(), icone: primeiro.icone, ret: Ret { x, y, w: cw, h: ch }, n: idx.len() });
                    for (j, &i) in idx.iter().enumerate() {
                        let (c, l) = ((j % cols) as f32, (j / cols) as f32);
                        itens[i].alvo = Ret { x: x + pad + c * (lado + folga), y: y + cab + l * (lado + titulo + folga), w: lado, h: lado };
                    }
                    x += cw + folga_g;
                }
                y += lh + folga_g;
            }
            (total, mais_larga)
        };

        // Tamanho escolhido nos ajustes; no mínimo, ou se nem a menor miniatura couber, vira lista.
        let minimo = 72.0 * s;
        if dimensionar {
            self.lista = self.pref_tamanho <= LIMITE_LISTA;
            self.lista_t = (self.pref_tamanho / LIMITE_LISTA).clamp(0.0, 1.0);
        }
        if self.lista {
            return self.organizar_lista(dimensionar);
        }
        let preferido = (minimo + (self.pref_tamanho - LIMITE_LISTA) / (1.0 - LIMITE_LISTA) * (280.0 * s - minimo)).min(self.monitor.h * 0.4);
        // maior miniatura que cabe (na busca, pode crescer um pouco além da de abertura)
        let mut lado = if dimensionar { preferido } else { (self.lado_inicial * 1.3).max(minimo) };
        let (total, larga) = loop {
            let (total, larga) = posicionar(lado, largura_max, 0.0, 0.0, &mut self.itens, &mut self.grupos);
            if total <= altura_max || lado <= minimo {
                break (total, larga);
            }
            lado -= 6.0 * s;
        };
        if dimensionar && total > altura_max {
            // janelas demais até para a menor miniatura: lista média
            self.lista = true;
            self.lista_t = 0.5;
            return self.organizar_lista(true);
        }
        if dimensionar {
            let pw = (larga.max(520.0 * s) + 2.0 * margem).ceil();
            let ph = (topo + total.max(120.0 * s) + rodape).ceil();
            if !self.garantir_dib(pw as usize, ph as usize) {
                crate::reg!("Alt+Tab: não consegui criar a imagem do painel ({pw}x{ph})");
                return false;
            }
            let x = (self.monitor.x + (self.monitor.w - pw) / 2.0).round();
            let y = (self.monitor.y + (self.monitor.h - ph) / 2.0).round();
            self.pos = (x, y);
            unsafe {
                SetWindowPos(self.hwnd, HWND_TOPMOST, x as i32, y as i32, pw as i32, ph as i32, SWP_NOACTIVATE | SWP_NOOWNERZORDER);
            }
            self.lado_inicial = lado;
        }
        let (pw, ph) = (self.w as f32, self.h as f32);
        let area = ph - topo - rodape;
        posicionar(lado, pw - 2.0 * margem, margem, topo + ((area - total) / 2.0).max(0.0), &mut self.itens, &mut self.grupos);
        for item in &mut self.itens {
            item.ret.mirar(item.alvo);
            item.opac.alvo = if item.visivel { 1.0 } else { 0.0 };
        }
        let lb = (420.0 * s).min(pw - 2.0 * margem);
        self.busca = Ret { x: (pw - lb) / 2.0, y: 18.0 * s, w: lb, h: 36.0 * s };
        self.limite = Ret { x: 8.0 * s, y: 8.0 * s, w: pw - 16.0 * s, h: ph - 16.0 * s };
        self.sujo_tudo = true;
        true
    }

    /// Lista: uma linha por janela, na ordem de uso, com o ícone e o nome inteiro; quando
    /// não cabe numa coluna, vira mais colunas lado a lado (preenchendo de cima para baixo).
    fn organizar_lista(&mut self, dimensionar: bool) -> bool {
        let s = self.escala;
        let (margem, topo, rodape) = (22.0 * s, 70.0 * s, 22.0 * s);
        let (linha, folga, _, fonte) = self.medidas_lista();
        let entre_colunas = 14.0 * s;
        let vis: Vec<usize> = (0..self.itens.len()).filter(|&i| self.itens[i].visivel).collect();
        let n = vis.len().max(1);
        let (largura_max, altura_max) = if dimensionar {
            (self.monitor.w * self.pref_painel - 2.0 * margem, self.monitor.h * self.pref_painel * 0.95 - topo - rodape)
        } else {
            (self.w as f32 - 2.0 * margem, self.h as f32 - topo - rodape)
        };
        let por_coluna_max = (((altura_max + folga) / (linha + folga)).floor() as usize).max(1);
        let colunas = n.div_ceil(por_coluna_max).max(1);
        let mut largura_coluna = ((560.0 + 80.0 * self.lista_t) * s).min((largura_max - (colunas - 1) as f32 * entre_colunas) / colunas as f32).max(260.0 * s);
        let por_coluna = n.div_ceil(colunas);
        if dimensionar {
            let conteudo_w = colunas as f32 * largura_coluna + (colunas - 1) as f32 * entre_colunas;
            let pw = (conteudo_w.max(520.0 * s) + 2.0 * margem).ceil();
            let ph = (topo + por_coluna as f32 * (linha + folga) - folga + rodape).max(topo + 120.0 * s + rodape).ceil();
            if !self.garantir_dib(pw as usize, ph as usize) {
                crate::reg!("Alt+Tab: não consegui criar a imagem do painel ({pw}x{ph})");
                return false;
            }
            let x = (self.monitor.x + (self.monitor.w - pw) / 2.0).round();
            let y = (self.monitor.y + (self.monitor.h - ph) / 2.0).round();
            self.pos = (x, y);
            unsafe {
                SetWindowPos(self.hwnd, HWND_TOPMOST, x as i32, y as i32, pw as i32, ph as i32, SWP_NOACTIVATE | SWP_NOOWNERZORDER);
            }
        } else {
            largura_coluna = largura_coluna.min((self.w as f32 - 2.0 * margem - (colunas - 1) as f32 * entre_colunas) / colunas as f32);
        }
        let (pw, ph) = (self.w as f32, self.h as f32);
        let conteudo_w = colunas as f32 * largura_coluna + (colunas - 1) as f32 * entre_colunas;
        let x0 = (pw - conteudo_w) / 2.0;
        for (j, &i) in vis.iter().enumerate() {
            let (c, l) = ((j / por_coluna) as f32, (j % por_coluna) as f32);
            self.itens[i].alvo = Ret { x: x0 + c * (largura_coluna + entre_colunas), y: topo + l * (linha + folga), w: largura_coluna, h: linha };
        }
        self.grupos.clear();
        // coluna dos nomes dos apps: do tamanho do maior, sem passar de um terço da linha
        let maior = vis.iter().map(|&i| self.texto.as_ref().map_or(self.itens[i].janela.app.chars().count() as f32 * fonte * 0.6, |t| t.medir(&self.itens[i].janela.app, fonte, true))).fold(0.0, f32::max);
        self.lista_app_w = (maior + 2.0 * s).min(largura_coluna * 0.34);
        for item in &mut self.itens {
            item.ret.mirar(item.alvo);
            item.opac.alvo = if item.visivel { 1.0 } else { 0.0 };
        }
        let lb = (420.0 * s).min(pw - 2.0 * margem);
        self.busca = Ret { x: (pw - lb) / 2.0, y: 18.0 * s, w: lb, h: 36.0 * s };
        self.limite = Ret { x: 8.0 * s, y: 8.0 * s, w: pw - 16.0 * s, h: ph - 16.0 * s };
        self.sujo_tudo = true;
        true
    }

    /// Medidas da lista conforme o ajuste de tamanho: (altura da linha, folga, ícone, fonte).
    fn medidas_lista(&self) -> (f32, f32, f32, f32) {
        let (s, t) = (self.escala, self.lista_t);
        let entre = |a: f32, b: f32| (a + (b - a) * t) * s;
        (entre(28.0, 44.0), entre(2.0, 6.0), entre(18.0, 32.0), entre(12.5, 13.5))
    }

    fn raio_lista(&self) -> f32 {
        (6.0 + 4.0 * self.lista_t) * self.escala
    }

    /// Ícone (x, y, lado) e textos de uma linha da lista desenhada em `a`. O realce usa o
    /// mesmo para redesenhar a linha selecionada em branco sobre a faixa da seleção.
    fn linha_lista(&self, k: usize, a: Ret, cor_app: u32, cor_titulo: u32, sombra: u32) -> ((f32, f32, f32), [Rotulo; 2]) {
        let s = self.escala;
        let (_, _, lado, fonte) = self.medidas_lista();
        let janela = &self.itens[k].janela;
        let pad = (8.0 + 2.0 * self.lista_t) * s;
        let yc = a.y + a.h / 2.0;
        let xi = a.x + pad;
        let xa = xi + lado + 10.0 * s;
        let xt = xa + self.lista_app_w + 16.0 * s;
        let fim = a.x + a.w - pad;
        let mut titulo = if janela.titulo == janela.app { String::new() } else { janela.titulo.clone() };
        if janela.minimizada {
            titulo.push_str("   (minimizada)");
        }
        let alto = fonte * 1.9;
        (
            (xi, yc - lado / 2.0, lado),
            [
                Rotulo {
                    texto: janela.app.clone(),
                    r: Ret { x: xa, y: yc - alto / 2.0, w: self.lista_app_w.min(fim - xa).max(1.0), h: alto },
                    tam: fonte,
                    semi: true,
                    cor: cor_app,
                    alinhar: STRING_ALIGN_NEAR,
                    sombra,
                },
                Rotulo { texto: titulo, r: Ret { x: xt, y: yc - alto / 2.0, w: (fim - xt).max(1.0), h: alto }, tam: fonte, semi: false, cor: cor_titulo, alinhar: STRING_ALIGN_NEAR, sombra: 0 },
            ],
        )
    }

    fn renderizar_base(&mut self) {
        let s = self.escala;
        let (w, h) = (self.w, self.h);
        let tema = &self.tema;
        let todo = Ret { x: 0.0, y: 0.0, w: w as f32, h: h as f32 };
        // fundo: véu de cor e, por cima, o branco do fosco (pré-multiplicado)
        let (v, t) = (tema.veu, tema.tinta);
        let fundo = [0, 1, 2].map(|k| t[k] * t[3] + v[k] * v[3] * (1.0 - t[3]));
        self.base.fill(empacotar([fundo[0], fundo[1], fundo[2], t[3] + v[3] * (1.0 - t[3])]));
        let mut rotulos: Vec<Rotulo> = Vec::new();
        let cache = &mut self.cores;
        let cores: Vec<Option<[f32; 3]>> = self.grupos.iter().map(|g| *cache.entry(g.icone).or_insert_with(|| cor_do_icone(g.icone))).collect();
        // lista: cor do ícone de cada linha
        let cores_linhas: Vec<(usize, Option<[f32; 3]>)> = if self.lista {
            self.itens
                .iter()
                .enumerate()
                .filter(|(_, i)| i.visivel)
                .map(|(k, i)| (k, *cache.entry(i.janela.icone).or_insert_with(|| cor_do_icone(i.janela.icone))))
                .collect()
        } else {
            Vec::new()
        };
        let raio_lista = self.raio_lista();
        let linhas: Vec<(Ret, Option<[f32; 3]>, HANDLE, (f32, f32, f32), [Rotulo; 2])> = cores_linhas
            .into_iter()
            .map(|(k, cor)| {
                let a = self.itens[k].alvo;
                let (icone, textos) = self.linha_lista(k, a, self.tema.texto, self.tema.texto2, self.tema.sombra_texto);
                (a, cor, self.itens[k].janela.icone, icone, textos)
            })
            .collect();
        let tema = &self.tema;
        {
            let mut tela = Tela { px: &mut self.base, w, h, recorte: todo };
            // vidro: brilho que desce do topo e borda iluminada
            tela.brilho(todo, 8.0 * s, tema.brilho, 0.4);
            tela.contornar_iluminado(todo.crescer(-0.5), 8.0 * s, 1.0 * s, tema.borda, 0.25);
            // busca
            tela.preencher(self.busca, self.busca.h / 2.0, tema.cartao);
            tela.contornar_iluminado(self.busca, self.busca.h / 2.0, 1.0 * s, tema.cartao_borda, 0.5);
            let n_vis = self.itens.iter().filter(|i| i.visivel).count();
            let (texto_busca, cor_busca) = if self.filtro.is_empty() {
                let dica = if self.fixo { "Digite para buscar   ·   Enter abre   ·   Esc fecha" } else { "Digite para buscar" };
                (dica.to_string(), tema.texto3)
            } else {
                (format!("{}   ·   {}", self.filtro, if n_vis == 0 { "nada".to_string() } else { n_vis.to_string() }), tema.texto)
            };
            rotulos.push(Rotulo { texto: format!("⌕  {texto_busca}"), r: self.busca.crescer(-10.0 * s), tam: 14.0 * s, semi: false, cor: cor_busca, alinhar: STRING_ALIGN_CENTER, sombra: 0 });

            for (g, cor) in self.grupos.iter().zip(&cores) {
                let raio = 16.0 * s;
                tela.preencher(g.ret, raio, tema.cartao);
                // tom do ícone só no topo do cartão, sumindo até o fim do cabeçalho
                if let Some(c) = cor.filter(|_| tema.tinta_cabecalho > 0.005) {
                    tela.brilho(g.ret, raio, [c[0], c[1], c[2], tema.tinta_cabecalho], (CABECALHO * s / g.ret.h).min(0.6));
                }
                tela.contornar_iluminado(g.ret, raio, 1.0 * s, tema.cartao_borda, 0.45);
                let lado = 30.0 * s;
                let (xi, yc) = (g.ret.x + 14.0 * s, g.ret.y + CABECALHO * s / 2.0 + 1.0 * s);
                tela.icone(g.icone, xi, yc - lado / 2.0, lado);
                let contagem = if g.n > 1 { 30.0 * s } else { 0.0 };
                let xt = xi + lado + 10.0 * s;
                rotulos.push(Rotulo {
                    texto: g.app.clone(),
                    r: Ret { x: xt, y: yc - 13.0 * s, w: g.ret.x + g.ret.w - 14.0 * s - contagem - xt, h: 26.0 * s },
                    tam: 14.5 * s,
                    semi: true,
                    cor: tema.texto,
                    alinhar: STRING_ALIGN_NEAR,
                    sombra: tema.sombra_texto,
                });
                if g.n > 1 {
                    // quantas janelas, numa pílula na cor do cartão
                    let p = Ret { x: g.ret.x + g.ret.w - 12.0 * s - 24.0 * s, y: yc - 10.0 * s, w: 24.0 * s, h: 20.0 * s };
                    let (c, a) = match cor {
                        Some(c) if tema.tinta_cabecalho > 0.005 => (*c, (tema.tinta_cabecalho * 1.6).max(0.08)),
                        _ => ([1.0, 1.0, 1.0], 0.08),
                    };
                    tela.preencher(p, 10.0 * s, [c[0], c[1], c[2], a]);
                    rotulos.push(Rotulo { texto: g.n.to_string(), r: p, tam: 12.0 * s, semi: true, cor: tema.texto2, alinhar: STRING_ALIGN_CENTER, sombra: 0 });
                }
            }
            // lista: ícone, nome do app e o título inteiro da janela, em colunas. Compacta, é
            // só texto sobre o vidro, como um menu; maior, cada linha ganha fundo na cor do ícone.
            let lt = self.lista_t;
            for (a, cor, icone, (xi, yi, lado), textos) in linhas {
                tela.preencher(a, raio_lista, [tema.cartao[0], tema.cartao[1], tema.cartao[2], tema.cartao[3] * lt]);
                if let Some(c) = cor.filter(|_| tema.tinta_cabecalho > 0.005) {
                    tela.preencher(a, raio_lista, [c[0], c[1], c[2], tema.tinta_cabecalho * 0.45 * lt]);
                }
                tela.icone(icone, xi, yi, lado);
                rotulos.extend(textos);
            }
            for item in self.itens.iter().filter(|i| i.visivel && !self.lista) {
                let a = item.alvo;
                if item.thumb == 0 {
                    // minimizada (ou sem miniatura): ícone grande numa vaga de vidro
                    tela.preencher(a, 10.0 * s, tema.vaga);
                    tela.contornar(a, 10.0 * s, 1.0 * s, tema.cartao_borda);
                    let lado = (48.0 * s).min(a.h * 0.4);
                    let c = a.centro();
                    tela.icone(item.janela.icone, c.x - lado / 2.0, c.y - lado / 2.0 - 8.0 * s, lado);
                    if item.janela.minimizada {
                        rotulos.push(Rotulo {
                            texto: "minimizada".into(),
                            r: Ret { x: a.x, y: c.y + lado / 2.0 - 2.0 * s, w: a.w, h: 18.0 * s },
                            tam: 11.0 * s,
                            semi: false,
                            cor: tema.texto3,
                            alinhar: STRING_ALIGN_CENTER, sombra: 0,
                        });
                    }
                }
                rotulos.push(Rotulo {
                    texto: item.janela.titulo.clone(),
                    r: Ret { x: a.x - 3.0 * s, y: a.y + a.h + 5.0 * s, w: a.w + 6.0 * s, h: 34.0 * s },
                    tam: 11.5 * s,
                    semi: false,
                    cor: tema.texto2,
                    alinhar: CENTRO_DUAS_LINHAS, sombra: 0,
                });
            }
            if n_vis == 0 {
                rotulos.push(Rotulo {
                    texto: format!("Nenhuma janela com “{}”", self.filtro),
                    r: Ret { x: 0.0, y: h as f32 / 2.0, w: w as f32, h: 30.0 * s },
                    tam: 15.0 * s,
                    semi: false,
                    cor: tema.texto2,
                    alinhar: STRING_ALIGN_CENTER, sombra: 0,
                });
            }
        }
        if let Some(t) = &self.texto {
            t.pintar(&mut self.base, w, h, &rotulos);
        }
    }

    /// Prévia da janela inteira: bem maior que o quadrado, na proporção da janela,
    /// centrada no quadrado (e depois empurrada para dentro do painel).
    fn previa(&self, b: Ret, proporcao: f32) -> Ret {
        let s = self.escala;
        let cw = (b.w * 2.7).max(b.w + 260.0 * s).min(self.limite.w);
        let ch = (b.h * 2.0).max(b.h + 170.0 * s).min(self.limite.h);
        let c = b.centro();
        Ret { x: c.x - cw / 2.0, y: c.y - ch / 2.0, w: cw, h: ch }.encaixar(proporcao)
    }

    /// Onde a miniatura aparece e que parte da janela ela mostra. Fechada, é o quadrado
    /// com a imagem recortada (o canto de cima à esquerda, onde ficam título, abas e
    /// menus); abrindo, o recorte e o quadrado crescem juntos até a janela inteira, sem
    /// nunca esticar a imagem nem ampliar além do tamanho real da janela.
    fn geometria(&self, i: usize) -> (Ret, Option<RECT>) {
        let item = &self.itens[i];
        let b = item.ret.atual();
        let t = item.abre.x.max(0.0);
        let (sw, sh) = item.fonte;
        if self.lista {
            return (b, None);
        }
        if item.thumb == 0 || sw < 1.0 || sh < 1.0 {
            return (b.escalar(1.0 + 0.05 * t).dentro_de(&self.limite), None);
        }
        let tc = t.min(1.0);
        let lado = sw.min(sh);
        let fonte = Ret { x: 0.0, y: 0.0, w: lado + (sw - lado) * tc, h: lado + (sh - lado) * tc };
        let k0 = b.w / lado;
        let k1 = (self.previa(b, sw / sh).w / sw).min(1.0f32.max(1.15 * k0));
        let k = k0 + (k1 - k0) * t;
        let (w, h) = (fonte.w * k, fonte.h * k);
        let c = b.centro();
        (Ret { x: c.x - w / 2.0, y: c.y - h / 2.0, w, h }.dentro_de(&self.limite), Some(fonte.inteiro()))
    }

    /// Anel da seleção: a mola leva o anel de uma miniatura a outra; por cima disso ele
    /// acompanha exatamente o que a miniatura selecionada faz (abrir a prévia, entrar).
    fn anel_na_tela(&self) -> Option<Ret> {
        let item = self.itens.get(self.sel).filter(|i| i.visivel)?;
        let mola = self.anel.atual();
        let fechada = item.alvo;
        let atual = self.geometria(self.sel).0;
        Some(Ret {
            x: mola.x + atual.x - fechada.x,
            y: mola.y + atual.y - fechada.y,
            w: mola.w + atual.w - fechada.w,
            h: mola.h + atual.h - fechada.h,
        })
    }

    /// Um quadro de animação: molas, miniaturas e a camada de destaque.
    pub fn quadro(&mut self) {
        if !self.aberto {
            return;
        }
        if !self.mostrado {
            if self.aberto_em.elapsed() >= ATRASO_MOSTRAR {
                self.mostrar();
            }
            return;
        }
        let agora = Instant::now();
        let dt = (agora - self.t_ant).as_secs_f32().clamp(0.0, 0.05);
        self.t_ant = agora;

        // a prévia só abre depois de parar um instante sobre a miniatura
        let foco = self.hover.filter(|&i| !self.lista && self.mouse_mexeu && self.itens.get(i).is_some_and(|it| it.visivel) && self.hover_desde.elapsed() >= ESPERA_PREVIA);
        if foco != self.trazido {
            if let Some(i) = foco {
                self.trazer_para_frente(i);
            }
            self.trazido = foco;
        }
        let previa_aberta = foco.is_some_and(|i| self.itens[i].thumb != 0);
        for (i, item) in self.itens.iter_mut().enumerate() {
            item.abre.alvo = if foco == Some(i) { 1.0 } else { 0.0 };
            // com uma prévia aberta, as outras janelas recuam um pouco
            item.opac.alvo = if !item.visivel {
                0.0
            } else if previa_aberta && foco != Some(i) {
                0.5
            } else {
                1.0
            };
            item.ret.passo(dt, 9.0, 0.82);
            item.abre.passo(dt, 6.5, 0.8);
            item.opac.passo(dt, 7.0, 1.0);
        }
        // o anel desliza com mola de uma miniatura para outra (ver `anel_na_tela`)
        if let Some(item) = self.itens.get(self.sel).filter(|i| i.visivel) {
            self.anel.mirar(item.alvo);
        }
        self.anel.passo(dt, 15.0, 0.86);

        // miniaturas ao vivo
        for i in 0..self.itens.len() {
            let item = &self.itens[i];
            if item.thumb == 0 {
                continue;
            }
            let (r, fonte) = self.geometria(i);
            // na lista não há miniaturas
            let opac = if self.lista { 0.0 } else { item.opac.x.clamp(0.0, 1.0) };
            let mut flags = DWM_TNP_RECTDESTINATION | DWM_TNP_OPACITY | DWM_TNP_VISIBLE | DWM_TNP_SOURCECLIENTAREAONLY;
            if fonte.is_some() {
                flags |= DWM_TNP_RECTSOURCE;
            }
            let p = DWM_THUMBNAIL_PROPERTIES {
                dwFlags: flags,
                rcDestination: r.inteiro(),
                rcSource: fonte.unwrap_or_default(),
                opacity: (opac * 255.0) as u8,
                fVisible: (opac > 0.01) as i32,
                fSourceClientAreaOnly: 0,
            };
            unsafe {
                DwmUpdateThumbnailProperties(item.thumb, &p);
            }
        }

        if self.sujo_tudo {
            self.desenhar();
            self.sujo_tudo = false;
        }
        self.atualizar_realce();
    }

    /// Fundo do painel: vidro, cartões, títulos (só muda quando a lista muda).
    fn desenhar(&mut self) {
        if self.bits.is_null() {
            return;
        }
        let quadro = unsafe { std::slice::from_raw_parts_mut(self.bits, self.w * self.h) };
        quadro.copy_from_slice(&self.base);
        unsafe {
            let dc = GetDC(self.hwnd);
            BitBlt(dc, 0, 0, self.w as i32, self.h as i32, self.dc, 0, 0, SRCCOPY);
            ReleaseDC(self.hwnd, dc);
        }
    }

    /// Anel da seleção, sombra e título da prévia, na camada por cima das miniaturas.
    fn atualizar_realce(&mut self) {
        let Some(mut realce) = self.realce.take() else { return };
        let s = self.escala;
        // na lista a seleção é a faixa da linha inteira; na grade, um anel em volta
        let anel = self.anel_na_tela().map(|a| if self.lista { a } else { a.crescer(5.0 * s) });
        // a prévia mais aberta (uma pode estar fechando enquanto outra abre)
        let previa = (0..self.itens.len())
            .filter(|&i| self.itens[i].thumb != 0 && self.itens[i].visivel && self.itens[i].abre.x > 0.02)
            .max_by(|&a, &b| self.itens[a].abre.x.total_cmp(&self.itens[b].abre.x))
            .map(|i| (i, self.geometria(i).0, self.itens[i].abre.x.min(1.0)));
        let painel = Ret { x: 0.0, y: 0.0, w: self.w as f32, h: self.h as f32 };
        let mut area = anel.map(|a| a.crescer(14.0 * s));
        if let Some((_, r, _)) = previa {
            let sombra = Ret { y: r.y + 10.0 * s, ..r }.crescer(32.0 * s).uniao(&r);
            area = Some(area.map_or(sombra, |a| a.uniao(&sombra)));
        }
        let Some(area) = area.and_then(|a| a.cortar(&painel)) else {
            realce.esconder();
            self.realce = Some(realce);
            return;
        };
        let (x0, y0) = (area.x.floor(), area.y.floor());
        let (rw, rh) = (((area.x + area.w).ceil() - x0) as usize, ((area.y + area.h).ceil() - y0) as usize);
        let q = |v: f32| (v * 4.0).round() as i32;
        let mut assinatura = vec![x0 as i32, y0 as i32, rw as i32, rh as i32, self.sel as i32];
        if let Some(a) = anel {
            assinatura.extend([q(a.x), q(a.y), q(a.w), q(a.h)]);
        }
        if let Some((i, r, t)) = previa {
            assinatura.extend([i as i32, q(r.x), q(r.y), q(r.w), q(r.h), (t * 500.0) as i32]);
        }
        if assinatura == realce.assinatura && realce.visivel {
            self.realce = Some(realce);
            return;
        }
        realce.assinatura = assinatura;

        let mut px = std::mem::take(&mut realce.px);
        px.clear();
        px.resize(rw * rh, 0);
        let desloca = |r: Ret| Ret { x: r.x - x0, y: r.y - y0, ..r };
        let mut rotulos: Vec<Rotulo> = Vec::new();
        {
            let mut tela = Tela { px: &mut px, w: rw, h: rh, recorte: Ret { x: 0.0, y: 0.0, w: rw as f32, h: rh as f32 } };
            if let Some((i, r, t)) = previa {
                let r = desloca(r);
                tela.sombra_externa(r, 3.0 * s, 26.0 * s, 10.0 * s, [0.0, 0.0, 0.0, 0.55 * t]);
                // aresta de luz em volta da imagem
                tela.contornar(r.crescer(-0.5), 2.0 * s, 1.0 * s, [1.0, 1.0, 1.0, 0.18 * t]);
                // título da janela numa etiqueta sobre a parte de baixo da prévia
                let a = smoothstep(0.6, 1.0, t);
                if a > 0.01 && r.w > 120.0 * s && r.h > 80.0 * s {
                    let titulo = self.itens[i].janela.titulo.clone();
                    let tam = 12.5 * s;
                    let medida = self.texto.as_ref().map_or(titulo.chars().count() as f32 * tam * 0.55, |t| t.medir(&titulo, tam, false));
                    let larg = (medida + 30.0 * s).min(r.w - 24.0 * s);
                    let e = Ret { x: r.x + (r.w - larg) / 2.0, y: r.y + r.h - 40.0 * s, w: larg, h: 28.0 * s };
                    let cor = self.tema.etiqueta;
                    tela.preencher(e, 14.0 * s, [cor[0], cor[1], cor[2], cor[3] * a]);
                    tela.contornar(e, 14.0 * s, 1.0 * s, [1.0, 1.0, 1.0, 0.10 * a]);
                    rotulos.push(Rotulo {
                        texto: titulo,
                        r: Ret { x: e.x + 12.0 * s, y: e.y, w: e.w - 24.0 * s, h: e.h },
                        tam,
                        semi: false,
                        cor: com_alfa(self.tema.texto, a),
                        alinhar: STRING_ALIGN_CENTER, sombra: 0,
                    });
                }
            }
            if let Some(an) = anel {
                let an = desloca(an);
                let d = self.tema.destaque;
                if self.lista {
                    // faixa cheia na cor de destaque, com a linha redesenhada em branco por cima
                    let raio = self.raio_lista();
                    tela.sombra_externa(an, raio, 8.0 * s, 2.0 * s, [d[0], d[1], d[2], 0.30]);
                    tela.preencher(an, raio, [d[0], d[1], d[2], 0.92]);
                    tela.contornar_iluminado(an.crescer(-0.5), raio, 1.0 * s, [1.0, 1.0, 1.0, 0.35], 0.0);
                    if self.itens.get(self.sel).is_some_and(|i| i.visivel) {
                        let ((xi, yi, lado), textos) = self.linha_lista(self.sel, an, 0xFFFF_FFFF, 0xE6FF_FFFF, 0);
                        tela.icone(self.itens[self.sel].janela.icone, xi, yi, lado);
                        rotulos.extend(textos);
                    }
                } else {
                    tela.sombra_externa(an, 9.0 * s, 9.0 * s, 0.0, [d[0], d[1], d[2], 0.28]);
                    tela.contornar(an, 9.0 * s, 2.25 * s, d);
                }
            }
        }
        if let Some(t) = &self.texto {
            t.pintar(&mut px, rw, rh, &rotulos);
        }
        realce.px = px;
        realce.apresentar(self.pos.0 as i32 + x0 as i32, self.pos.1 as i32 + y0 as i32, rw, rh);
        self.realce = Some(realce);
    }

    /// Repinta tudo a partir do quadro atual (pedido do Windows, WM_PAINT).
    pub fn repintar(&self, dc: HANDLE) {
        if self.bits.is_null() {
            return;
        }
        unsafe {
            BitBlt(dc, 0, 0, self.w as i32, self.h as i32, self.dc, 0, 0, SRCCOPY);
        }
    }

    fn indice_em(&self, x: f32, y: f32) -> Option<usize> {
        if let Some(h) = self.hover.filter(|&h| h < self.itens.len() && self.itens[h].visivel) {
            if self.geometria(h).0.contem(x, y) {
                return Some(h);
            }
        }
        (0..self.itens.len()).find(|&i| self.itens[i].visivel && self.geometria(i).0.contem(x, y))
    }

    /// Passa a miniatura para cima das outras (a última registrada fica por cima).
    fn trazer_para_frente(&mut self, i: usize) {
        let item = &mut self.itens[i];
        if item.thumb == 0 {
            return;
        }
        unsafe {
            DwmUnregisterThumbnail(item.thumb);
            let mut t = 0isize;
            item.thumb = if DwmRegisterThumbnail(self.hwnd, item.janela.hwnd, &mut t) == 0 { t } else { 0 };
        }
    }

    pub fn mouse(&mut self, x: f32, y: f32) {
        // O Windows manda "mouse mexeu" quando o painel aparece embaixo do ponteiro;
        // só conta como movimento quando o ponteiro sai de onde estava ao abrir.
        if !self.mouse_mexeu {
            let mut pt = POINT::default();
            unsafe { GetCursorPos(&mut pt) };
            let (a, b) = (self.cursor_abertura, pt);
            self.mouse_mexeu = (a.x - b.x).abs() + (a.y - b.y).abs() > (8.0 * self.escala) as i32;
            if !self.mouse_mexeu {
                return;
            }
        }
        let novo = self.indice_em(x, y);
        if novo != self.hover {
            self.hover = novo;
            self.hover_desde = Instant::now();
            if let Some(i) = novo {
                self.sel = i;
            }
        }
    }

    pub fn clique(&mut self, x: f32, y: f32) -> Clique {
        match self.indice_em(x, y) {
            Some(i) => {
                self.sel = i;
                Clique::Item
            }
            None => Clique::Nada,
        }
    }

    pub fn indice_sob(&self, x: f32, y: f32) -> Option<usize> {
        self.indice_em(x, y)
    }

    /// Tab e Shift+Tab: andam na ordem de uso, como no Windows.
    pub fn avancar(&mut self, passo: i32) {
        let vis: Vec<usize> = (0..self.itens.len()).filter(|&i| self.itens[i].visivel).collect();
        if vis.is_empty() {
            return;
        }
        let pos = vis.iter().position(|&i| i == self.sel).unwrap_or(0) as i32;
        let n = vis.len() as i32;
        self.sel = vis[((pos + passo) % n + n) as usize % n as usize];
        self.hover = None;
    }

    /// Setas: vai para a janela mais próxima naquela direção.
    pub fn mover(&mut self, dx: f32, dy: f32) {
        let Some(atual) = self.itens.get(self.sel).map(|i| i.alvo.centro()) else { return };
        let mut melhor: Option<(f32, usize)> = None;
        for (i, item) in self.itens.iter().enumerate() {
            if !item.visivel || i == self.sel {
                continue;
            }
            let c = item.alvo.centro();
            let (vx, vy) = (c.x - atual.x, c.y - atual.y);
            let frente = vx * dx + vy * dy;
            if frente <= 1.0 {
                continue;
            }
            let lado = (vx * dy - vy * dx).abs();
            let custo = frente + 2.5 * lado;
            if melhor.is_none_or(|(m, _)| custo < m) {
                melhor = Some((custo, i));
            }
        }
        if let Some((_, i)) = melhor {
            self.sel = i;
            self.hover = None;
        }
    }

    pub fn filtrar(&mut self, filtro: String) {
        self.filtro = filtro;
        let f = normalizar(&self.filtro);
        for item in &mut self.itens {
            item.visivel = f.is_empty() || normalizar(&item.janela.titulo).contains(&f) || normalizar(&item.janela.app).contains(&f);
        }
        if !self.itens.get(self.sel).is_some_and(|i| i.visivel) {
            if let Some(i) = self.itens.iter().position(|i| i.visivel) {
                self.sel = i;
            }
        }
        self.hover = None;
        self.trazido = None;
        self.organizar(false);
        self.renderizar_base();
    }

    /// Tira da lista a janela selecionada (depois de pedir para ela fechar).
    pub fn remover_selecionada(&mut self) {
        if self.sel >= self.itens.len() {
            return;
        }
        let item = self.itens.remove(self.sel);
        if item.thumb != 0 {
            unsafe { DwmUnregisterThumbnail(item.thumb) };
        }
        let vis: Vec<usize> = (0..self.itens.len()).filter(|&i| self.itens[i].visivel).collect();
        self.sel = vis.iter().copied().find(|&i| i >= self.sel).or_else(|| vis.last().copied()).unwrap_or(0);
        self.hover = None;
        self.trazido = None;
        self.organizar(false);
        self.renderizar_base();
    }

    pub fn vazio(&self) -> bool {
        self.itens.is_empty()
    }

    /// Centro da miniatura na tela (para os testes da demonstração).
    pub fn centro_na_tela(&self, i: usize) -> Option<POINT> {
        let c = self.itens.get(i)?.alvo.centro();
        Some(POINT { x: (self.pos.0 + c.x) as i32, y: (self.pos.1 + c.y) as i32 })
    }
}

impl Drop for Painel {
    fn drop(&mut self) {
        self.realce = None;
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
