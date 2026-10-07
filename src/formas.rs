// Desenho vetorial dos ponteiros por campos de distância (SDF), com borda
// suavizada, contorno e sombra. Coordenadas locais em pixels a 100% de escala,
// com o ponto ativo (hotspot) do ponteiro na origem.
use crate::config::Estilo;
use std::f32::consts::{PI, TAU};
use std::ops::{Add, AddAssign, Div, Mul, Sub};

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct V2 {
    pub x: f32,
    pub y: f32,
}

pub const fn v2(x: f32, y: f32) -> V2 {
    V2 { x, y }
}

impl V2 {
    pub fn len(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
    pub fn dot(self, o: V2) -> f32 {
        self.x * o.x + self.y * o.y
    }
    pub fn abs(self) -> V2 {
        v2(self.x.abs(), self.y.abs())
    }
    pub fn lerp(self, o: V2, t: f32) -> V2 {
        self + (o - self) * t
    }
    /// Gira o ponto pelo ângulo (em telas, y para baixo: sentido horário).
    pub fn girar(self, ang: f32) -> V2 {
        let (s, c) = ang.sin_cos();
        v2(c * self.x - s * self.y, s * self.x + c * self.y)
    }
}

impl Add for V2 {
    type Output = V2;
    fn add(self, o: V2) -> V2 {
        v2(self.x + o.x, self.y + o.y)
    }
}
impl Sub for V2 {
    type Output = V2;
    fn sub(self, o: V2) -> V2 {
        v2(self.x - o.x, self.y - o.y)
    }
}
impl Mul<f32> for V2 {
    type Output = V2;
    fn mul(self, k: f32) -> V2 {
        v2(self.x * k, self.y * k)
    }
}
impl Div<f32> for V2 {
    type Output = V2;
    fn div(self, k: f32) -> V2 {
        v2(self.x / k, self.y / k)
    }
}
impl AddAssign for V2 {
    fn add_assign(&mut self, o: V2) {
        self.x += o.x;
        self.y += o.y;
    }
}

pub fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// ---------- Primitivas de distância (negativo = dentro) ----------

fn sd_poligono(p: V2, v: &[V2]) -> f32 {
    let mut d = (p - v[0]).dot(p - v[0]);
    let mut s = 1.0;
    let mut j = v.len() - 1;
    for i in 0..v.len() {
        let e = v[j] - v[i];
        let w = p - v[i];
        let b = w - e * (w.dot(e) / e.dot(e)).clamp(0.0, 1.0);
        d = d.min(b.dot(b));
        let c1 = p.y >= v[i].y;
        let c2 = p.y < v[j].y;
        let c3 = e.x * w.y > e.y * w.x;
        if (c1 && c2 && c3) || (!c1 && !c2 && !c3) {
            s = -s;
        }
        j = i;
    }
    s * d.sqrt()
}

fn sd_capsula(p: V2, a: V2, b: V2, r: f32) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = (pa.dot(ba) / ba.dot(ba)).clamp(0.0, 1.0);
    (pa - ba * h).len() - r
}

fn sd_caixa(p: V2, centro: V2, meia: V2, r: f32) -> f32 {
    let q = (p - centro).abs() - meia + v2(r, r);
    v2(q.x.max(0.0), q.y.max(0.0)).len() + q.x.max(q.y).min(0.0) - r
}

/// União suave: junta as formas com uma curva no encontro.
fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    b + (a - b) * h - k * h * (1.0 - h)
}

// ---------- Tipos de ponteiro ----------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tipo {
    Seta,
    Texto,
    Mao,
    Espera,
    SetaEspera,
    Cruz,
    Proibido,
    RedimH,
    RedimV,
    RedimNwse,
    RedimNesw,
    Mover,
}

impl Tipo {
    pub const TODOS: [Tipo; 12] = [
        Tipo::Seta,
        Tipo::Mao,
        Tipo::Texto,
        Tipo::Espera,
        Tipo::SetaEspera,
        Tipo::RedimH,
        Tipo::RedimV,
        Tipo::RedimNwse,
        Tipo::RedimNesw,
        Tipo::Mover,
        Tipo::Cruz,
        Tipo::Proibido,
    ];

    /// Tem animação própria (precisa redesenhar todo quadro).
    pub fn animado(self) -> bool {
        matches!(self, Tipo::Espera | Tipo::SetaEspera)
    }
}

pub type Cor = [f32; 4];

pub struct Paleta {
    pub preenchimento: Cor,
    pub contorno: Cor,
    pub largura: f32,
}

pub fn paleta(estilo: Estilo) -> Paleta {
    match estilo {
        Estilo::Apple => Paleta { preenchimento: [0.03, 0.03, 0.035, 1.0], contorno: [1.0, 1.0, 1.0, 1.0], largura: 1.6 },
        Estilo::Claro => Paleta { preenchimento: [1.0, 1.0, 1.0, 1.0], contorno: [0.0, 0.0, 0.0, 1.0], largura: 1.3 },
    }
}

type Sdf = Box<dyn Fn(V2) -> f32>;

struct Parte {
    sdf: Sdf,
    preenchimento: Cor,
    contorno: Cor,
    largura: f32,
    /// Modula a opacidade do preenchimento (usado no giro do "aguarde").
    alfa: Option<Sdf>,
    /// Entra no cálculo da sombra.
    sombra: bool,
}

const SETA: [V2; 7] = [
    v2(0.5, 0.5),
    v2(0.5, 17.5),
    v2(4.5, 13.8),
    v2(7.3, 20.1),
    v2(9.9, 19.0),
    v2(7.2, 12.9),
    v2(12.5, 12.9),
];

fn sd_mao(p: V2) -> f32 {
    let k = 0.92;
    let p = (p - v2(0.5, 0.0)) / k;
    let indicador = sd_capsula(p, v2(0.0, 2.8), v2(0.0, 12.0), 2.3);
    let medio = sd_capsula(p, v2(4.5, 9.6), v2(4.5, 13.5), 2.2);
    let anelar = sd_capsula(p, v2(8.7, 10.6), v2(8.7, 14.0), 2.1);
    let minimo = sd_capsula(p, v2(12.5, 12.2), v2(12.5, 15.0), 1.9);
    let palma = sd_caixa(p, v2(6.2, 17.3), v2(7.6, 5.0), 3.4);
    let polegar = sd_capsula(p, v2(-1.0, 17.0), v2(-4.6, 12.6), 2.2);
    let s = 0.8;
    let d = smin(smin(smin(smin(smin(indicador, medio, s), anelar, s), minimo, s), palma, s), polegar, s);
    d * k
}

fn sd_linhas_mao(p: V2) -> f32 {
    let k = 0.92;
    let p = (p - v2(0.5, 0.0)) / k;
    let a = sd_capsula(p, v2(2.35, 9.9), v2(2.35, 13.6), 0.42);
    let b = sd_capsula(p, v2(6.6, 11.0), v2(6.6, 14.2), 0.42);
    let c = sd_capsula(p, v2(10.65, 12.3), v2(10.65, 14.9), 0.42);
    a.min(b).min(c) * k
}

fn sd_texto(p: V2) -> f32 {
    let p = p - v2(0.5, 0.5);
    let haste = sd_capsula(p, v2(0.0, -7.2), v2(0.0, 7.2), 0.8);
    let r = 0.75;
    let orelhas = sd_capsula(p, v2(0.0, -7.4), v2(-2.9, -9.1), r)
        .min(sd_capsula(p, v2(0.0, -7.4), v2(2.9, -9.1), r))
        .min(sd_capsula(p, v2(0.0, 7.4), v2(-2.9, 9.1), r))
        .min(sd_capsula(p, v2(0.0, 7.4), v2(2.9, 9.1), r));
    haste.min(orelhas)
}

fn sd_seta_dupla(p: V2) -> f32 {
    let haste = sd_capsula(p, v2(-6.2, 0.0), v2(6.2, 0.0), 1.15);
    let esq = sd_poligono(p, &[v2(-10.8, 0.0), v2(-5.6, -4.9), v2(-5.6, 4.9)]);
    let dir = sd_poligono(p, &[v2(10.8, 0.0), v2(5.6, 4.9), v2(5.6, -4.9)]);
    haste.min(esq).min(dir)
}

fn redim(ang: f32) -> Sdf {
    Box::new(move |p: V2| sd_seta_dupla((p - v2(0.5, 0.5)).girar(-ang)))
}

fn sd_mover(p: V2) -> f32 {
    let k = 0.92;
    let p = (p - v2(0.5, 0.5)) / k;
    sd_seta_dupla(p).min(sd_seta_dupla(p.girar(-PI / 2.0))) * k
}

fn sd_cruz(p: V2) -> f32 {
    let p = p - v2(0.5, 0.5);
    let r = 0.75;
    sd_capsula(p, v2(-9.0, 0.0), v2(-2.6, 0.0), r)
        .min(sd_capsula(p, v2(2.6, 0.0), v2(9.0, 0.0), r))
        .min(sd_capsula(p, v2(0.0, -9.0), v2(0.0, -2.6), r))
        .min(sd_capsula(p, v2(0.0, 2.6), v2(0.0, 9.0), r))
        .min(p.len() - 1.0)
}

fn sd_proibido(p: V2) -> f32 {
    let p = p - v2(0.5, 0.5);
    let anel = (p.len() - 7.5).abs() - 1.45;
    anel.min(sd_capsula(p, v2(-5.3, -5.3), v2(5.3, 5.3), 1.45))
}

/// Anel do "aguarde": um arco escuro que gira sobre o anel claro.
fn giro(centro: V2, raio: f32, meia: f32, tempo: f32, pal: &Paleta) -> Parte {
    let fase = (tempo * 1.2).fract() * TAU;
    Parte {
        sdf: Box::new(move |p: V2| ((p - centro).len() - raio).abs() - meia),
        preenchimento: pal.preenchimento,
        contorno: pal.contorno,
        largura: 1.1,
        alfa: Some(Box::new(move |p: V2| {
            let q = p - centro;
            let g = ((q.y.atan2(q.x) - fase) / TAU).rem_euclid(1.0);
            0.12 + 0.88 * g * g
        })),
        sombra: true,
    }
}

fn partes(tipo: Tipo, estilo: Estilo, tempo: f32) -> Vec<Parte> {
    let pal = paleta(estilo);
    let simples = |sdf: Sdf, largura: f32| Parte {
        sdf,
        preenchimento: pal.preenchimento,
        contorno: pal.contorno,
        largura,
        alfa: None,
        sombra: true,
    };
    match tipo {
        Tipo::Seta => vec![simples(Box::new(|p| sd_poligono(p, &SETA)), pal.largura)],
        Tipo::SetaEspera => vec![
            simples(Box::new(|p| sd_poligono(p, &SETA)), pal.largura),
            giro(v2(16.5, 19.5), 4.4, 1.3, tempo, &pal),
        ],
        Tipo::Mao => {
            // A mão é branca com contorno preto nos dois estilos, como no macOS e no Windows.
            let branco = [1.0, 1.0, 1.0, 1.0];
            let preto = [0.03, 0.03, 0.035, 1.0];
            vec![
                Parte { sdf: Box::new(sd_mao), preenchimento: branco, contorno: preto, largura: 1.25, alfa: None, sombra: true },
                Parte { sdf: Box::new(sd_linhas_mao), preenchimento: preto, contorno: preto, largura: 0.0, alfa: None, sombra: false },
            ]
        }
        Tipo::Texto => vec![simples(Box::new(sd_texto), 1.2)],
        Tipo::Espera => vec![giro(v2(0.5, 0.5), 7.6, 1.7, tempo, &pal)],
        Tipo::RedimH => vec![simples(redim(0.0), 1.3)],
        Tipo::RedimV => vec![simples(redim(PI / 2.0), 1.3)],
        Tipo::RedimNwse => vec![simples(redim(PI / 4.0), 1.3)],
        Tipo::RedimNesw => vec![simples(redim(-PI / 4.0), 1.3)],
        Tipo::Mover => vec![simples(Box::new(sd_mover), 1.3)],
        Tipo::Cruz => vec![simples(Box::new(sd_cruz), 1.1)],
        Tipo::Proibido => vec![Parte {
            sdf: Box::new(sd_proibido),
            preenchimento: [0.92, 0.23, 0.2, 1.0],
            contorno: [1.0, 1.0, 1.0, 1.0],
            largura: 1.2,
            alfa: None,
            sombra: true,
        }],
    }
}

// ---------- Composição em alfa pré-multiplicado ----------

pub fn premul(c: Cor, cobertura: f32) -> Cor {
    let a = c[3] * cobertura;
    [c[0] * a, c[1] * a, c[2] * a, a]
}

/// `src` por cima de `dst`, as duas pré-multiplicadas.
pub fn sobre(src: Cor, dst: Cor) -> Cor {
    let k = 1.0 - src[3];
    [src[0] + dst[0] * k, src[1] + dst[1] * k, src[2] + dst[2] * k, src[3] + dst[3] * k]
}

struct Sombra {
    opacidade: f32,
    desloc: V2,
    raio: f32,
}

/// Cor de um pixel. `p` em coordenadas locais; `ps` é o ponto deslocado da sombra.
fn pixel(partes: &[Parte], p: V2, ps: V2, s: f32, sombra: &Sombra) -> Cor {
    let mut c = [0.0; 4];
    if sombra.opacidade > 0.0 {
        let mut sil = f32::MAX;
        for pt in partes.iter().filter(|x| x.sombra) {
            sil = sil.min((pt.sdf)(ps) * s - pt.largura * s);
        }
        let a = sombra.opacidade * (1.0 - smoothstep(-sombra.raio, sombra.raio, sil));
        c = [0.0, 0.0, 0.0, a];
    }
    for pt in partes {
        let d = (pt.sdf)(p) * s;
        if pt.largura > 0.0 {
            let cob = (0.5 - (d - pt.largura * s)).clamp(0.0, 1.0);
            if cob > 0.0 {
                c = sobre(premul(pt.contorno, cob), c);
            }
        }
        let mut cob = (0.5 - d).clamp(0.0, 1.0);
        if cob > 0.0 {
            if let Some(f) = &pt.alfa {
                cob *= f(p);
            }
            c = sobre(premul(pt.preenchimento, cob), c);
        }
    }
    c
}

/// Uma forma a desenhar no sprite (mais de uma durante a troca de forma).
pub struct Camada {
    pub tipo: Tipo,
    pub alfa: f32,
    pub escala: f32,
}

/// Imagem do ponteiro já pronta, com o ponto ativo em (ox, oy).
pub struct Sprite {
    pub w: usize,
    pub h: usize,
    pub ox: i32,
    pub oy: i32,
    pub px: Vec<Cor>,
    /// Índices dos pixels não transparentes (acelera o borrão).
    pub ativos: Vec<u32>,
}

impl Sprite {
    pub fn novo() -> Self {
        Sprite { w: 0, h: 0, ox: 0, oy: 0, px: Vec::new(), ativos: Vec::new() }
    }

    pub fn renderizar(&mut self, camadas: &[Camada], estilo: Estilo, sombra: f32, tempo: f32) {
        let s_max = camadas.iter().map(|c| c.escala).fold(0.1, f32::max);
        let lo = (-18.0 * s_max).floor() as i32 - 3;
        let hi = (30.0 * s_max).ceil() as i32 + 4;
        let lado = (hi - lo) as usize;
        self.w = lado;
        self.h = lado;
        self.ox = -lo;
        self.oy = -lo;
        self.px.clear();
        self.px.resize(lado * lado, [0.0; 4]);
        for cam in camadas {
            if cam.alfa <= 0.001 {
                continue;
            }
            let partes = partes(cam.tipo, estilo, tempo);
            let s = cam.escala;
            let sb = Sombra { opacidade: sombra / 100.0, desloc: v2(0.0, 1.3 * s), raio: 2.2 * s };
            for j in 0..lado {
                for i in 0..lado {
                    let ptela = v2(i as f32 + 0.5 - self.ox as f32, j as f32 + 0.5 - self.oy as f32);
                    let c = pixel(&partes, ptela / s, (ptela - sb.desloc) / s, s, &sb);
                    if c[3] > 0.0 {
                        let d = &mut self.px[j * lado + i];
                        for k in 0..4 {
                            d[k] += c[k] * cam.alfa;
                        }
                    }
                }
            }
        }
        self.ativos.clear();
        for (i, c) in self.px.iter().enumerate() {
            if c[3] > 1.0 / 1024.0 {
                self.ativos.push(i as u32);
            }
        }
    }
}

/// Ícone do programa (atalho): quadrado arredondado em degradê com a seta,
/// em RGBA não pré-multiplicado. Mesmo desenho do icone.svg da tela de ajustes.
pub fn icone_app(lado: usize) -> Vec<u8> {
    let n = lado as f32;
    let s = n / 64.0;
    let partes = partes(Tipo::Seta, Estilo::Apple, 0.0);
    let esc = 1.65 * s;
    let desloc = v2(22.0 * s, 14.0 * s);
    let sb = Sombra { opacidade: 0.45, desloc: v2(0.0, 1.2 * esc), raio: 1.6 * esc };
    let paradas = [(0.0, [0.353, 0.784, 0.98]), (0.55, [0.369, 0.361, 0.902]), (1.0, [0.749, 0.353, 0.949])];
    let mut out = vec![0u8; lado * lado * 4];
    for j in 0..lado {
        for i in 0..lado {
            let p = v2(i as f32 + 0.5, j as f32 + 0.5);
            let d = sd_caixa(p, v2(n / 2.0, n / 2.0), v2(30.0 * s, 30.0 * s), 15.0 * s);
            let mut c = [0.0; 4];
            let cob = (0.5 - d).clamp(0.0, 1.0);
            if cob > 0.0 {
                let u = ((p.x + p.y) / (2.0 * n)).clamp(0.0, 1.0);
                let k = if u < paradas[1].0 { 0 } else { 1 };
                let (a, b) = (paradas[k], paradas[k + 1]);
                let f = (u - a.0) / (b.0 - a.0);
                let mut cor = [0.0, 0.0, 0.0, 1.0];
                for q in 0..3 {
                    cor[q] = a.1[q] + (b.1[q] - a.1[q]) * f;
                }
                // brilho suave na metade de cima, como vidro
                let brilho = 0.16 * (1.0 - smoothstep(0.30 * n, 0.55 * n, p.y));
                for q in 0..3 {
                    cor[q] += (1.0 - cor[q]) * brilho;
                }
                c = premul(cor, cob);
            }
            let ps = p - desloc;
            c = sobre(pixel(&partes, ps / esc, (ps - sb.desloc) / esc, esc, &sb), c);
            let a = c[3].clamp(0.0, 1.0);
            let k = (j * lado + i) * 4;
            if a > 0.0 {
                for q in 0..3 {
                    out[k + q] = ((c[q] / a).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
            }
            out[k + 3] = (a * 255.0 + 0.5) as u8;
        }
    }
    out
}

/// Ícone da bandeja: a seta no estilo Apple, em BGRA não pré-multiplicado.
pub fn icone(lado: usize) -> Vec<u32> {
    let s = lado as f32 / 24.0;
    let partes = partes(Tipo::Seta, Estilo::Apple, 0.0);
    let sb = Sombra { opacidade: 0.0, desloc: v2(0.0, 0.0), raio: 1.0 };
    // centraliza a seta (caixa local aproximada 0..13 x 0..20)
    let desloc = v2((lado as f32 - 13.5 * s) / 2.0, (lado as f32 - 20.6 * s) / 2.0);
    let mut out = vec![0u32; lado * lado];
    for j in 0..lado {
        for i in 0..lado {
            let ptela = v2(i as f32 + 0.5, j as f32 + 0.5) - desloc;
            let c = pixel(&partes, ptela / s, ptela / s, s, &sb);
            let a = c[3].clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            let un = |x: f32| ((x / a).clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
            out[j * lado + i] = ((a * 255.0 + 0.5) as u32) << 24 | un(c[0]) << 16 | un(c[1]) << 8 | un(c[2]);
        }
    }
    out
}
