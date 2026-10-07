// Ferramentas de conferência, sem esconder o ponteiro de verdade:
//   --galeria <pasta>      imagens das formas e dos efeitos de movimento
//   --diagnostico <arq>    sincronia, cursores do sistema e desempenho do motor
use crate::config::{Config, Estilo};
use crate::ffi::*;
use crate::formas::{v2, Tipo, V2};
use crate::motor::{Motor, Quadro};
use crate::sistema::{self, MapaCursores, Vblank};
use std::fmt::Write as _;
use std::path::Path;
use std::time::{Duration, Instant};

struct Imagem {
    w: usize,
    h: usize,
    px: Vec<[f32; 3]>,
}

impl Imagem {
    fn nova(w: usize, h: usize) -> Self {
        Imagem { w, h, px: vec![[1.0; 3]; w * h] }
    }

    fn retangulo(&mut self, x0: usize, y0: usize, w: usize, h: usize, cor: [f32; 3]) {
        for y in y0..(y0 + h).min(self.h) {
            for x in x0..(x0 + w).min(self.w) {
                self.px[y * self.w + x] = cor;
            }
        }
    }

    /// Fundo em degradê colorido, para ver o ponteiro sobre "foto".
    fn degrade(&mut self, x0: usize, y0: usize, w: usize, h: usize) {
        for y in y0..(y0 + h).min(self.h) {
            for x in x0..(x0 + w).min(self.w) {
                let u = (x - x0) as f32 / w as f32;
                let v = (y - y0) as f32 / h as f32;
                self.px[y * self.w + x] = [0.15 + 0.8 * u, 0.35 + 0.4 * v, 0.85 - 0.6 * u];
            }
        }
    }

    fn colar(&mut self, q: &Quadro) {
        for j in 0..q.h {
            for i in 0..q.w {
                let (x, y) = (q.x + i as i32, q.y + j as i32);
                if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
                    continue;
                }
                let c = q.px[j * q.w + i];
                let a = (c >> 24) as f32 / 255.0;
                if a == 0.0 {
                    continue;
                }
                let canal = |s: u32| ((c >> s) & 0xFF) as f32 / 255.0;
                let d = &mut self.px[y as usize * self.w + x as usize];
                *d = [canal(16) + d[0] * (1.0 - a), canal(8) + d[1] * (1.0 - a), canal(0) + d[2] * (1.0 - a)];
            }
        }
    }

    fn salvar(&self, caminho: &Path, zoom: usize) {
        let (w, h) = (self.w * zoom, self.h * zoom);
        let mut rgba = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let c = self.px[(y / zoom) * self.w + x / zoom];
                let i = (y * w + x) * 4;
                rgba[i] = (c[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                rgba[i + 1] = (c[1].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                rgba[i + 2] = (c[2].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                rgba[i + 3] = 255;
            }
        }
        if let Err(e) = crate::png::salvar(caminho, w, h, &rgba) {
            eprintln!("não consegui salvar {}: {e}", caminho.display());
        }
    }
}

const BRANCO: [f32; 3] = [1.0, 1.0, 1.0];
const ESCURO: [f32; 3] = [0.12, 0.12, 0.13];
const DT: f64 = 1.0 / 164.0;

/// Desenha um ponteiro parado em (x, y).
fn parado(img: &mut Imagem, tipo: Tipo, x: f32, y: f32, cfg: &Config, tempo: f64) {
    let mut m = Motor::novo();
    if let Some(q) = m.quadro(tempo, v2(x, y), tipo, false, 1.0, cfg) {
        img.colar(&q);
    }
}

/// Simula um movimento; `caminho(t)` dá a posição do ponteiro real.
/// Cola na imagem o quadro do instante `t_fim`.
fn movimento(img: &mut Imagem, cfg: &Config, t_fim: f64, caminho: impl Fn(f64) -> V2, pressionar: Option<f64>) {
    let mut m = Motor::novo();
    let mut t = 0.0;
    while t < t_fim - DT * 0.5 {
        let p = pressionar.is_some_and(|tp| t >= tp && t < tp + 0.06);
        m.quadro(t, caminho(t), Tipo::Seta, p, 1.0, cfg);
        t += DT;
    }
    let p = pressionar.is_some_and(|tp| t >= tp && t < tp + 0.06);
    if let Some(q) = m.quadro(t, caminho(t), Tipo::Seta, p, 1.0, cfg) {
        img.colar(&q);
    }
}

pub fn gerar(pasta: &Path) {
    let _ = std::fs::create_dir_all(pasta);

    // 1) Formas paradas, nos dois estilos, sobre branco, escuro e degradê (ampliado 3x).
    let celula = 56usize;
    let colunas = 6;
    let tipos = Tipo::TODOS;
    let linhas_por_fundo = tipos.len().div_ceil(colunas);
    let fundos = 3;
    for estilo in [Estilo::Apple, Estilo::Claro] {
        let cfg = Config { estilo, ..Config::default() };
        let mut img = Imagem::nova(celula * colunas, celula * linhas_por_fundo * fundos);
        for f in 0..fundos {
            let y0 = f * celula * linhas_por_fundo;
            match f {
                0 => img.retangulo(0, y0, img.w, celula * linhas_por_fundo, BRANCO),
                1 => img.retangulo(0, y0, img.w, celula * linhas_por_fundo, ESCURO),
                _ => img.degrade(0, y0, img.w, celula * linhas_por_fundo),
            }
            for (i, &tipo) in tipos.iter().enumerate() {
                let (cx, cy) = (i % colunas, i / colunas);
                let centrado = !matches!(tipo, Tipo::Seta | Tipo::SetaEspera | Tipo::Mao);
                let (dx, dy) = if centrado { (28.0, 28.0) } else { (20.0, 16.0) };
                parado(&mut img, tipo, (cx * celula) as f32 + dx, (y0 + cy * celula) as f32 + dy, &cfg, 0.3);
            }
        }
        let nome = if estilo == Estilo::Apple { "formas_apple.png" } else { "formas_claro.png" };
        img.salvar(&pasta.join(nome), 3);
    }

    // 2) Movimento: retas em três velocidades, uma curva e o clique, sobre branco e escuro.
    let cfg = Config::default();
    let (largura, altura_linha) = (760usize, 90usize);
    let casos: Vec<(&str, f64)> = vec![("800 px/s", 800.0), ("2500 px/s", 2500.0), ("6000 px/s", 6000.0), ("curva", 0.0)];
    let mut img = Imagem::nova(largura, altura_linha * casos.len() * 2 + 140);
    for (k, (_, vel)) in casos.iter().enumerate() {
        for (f, cor) in [BRANCO, ESCURO].iter().enumerate() {
            let y0 = (k * 2 + f) * altura_linha;
            img.retangulo(0, y0, largura, altura_linha, *cor);
            let yc = (y0 + altura_linha / 2 - 8) as f32;
            if *vel > 0.0 {
                let v = *vel;
                let t_fim = (560.0 - 60.0) / v;
                movimento(&mut img, &cfg, t_fim, move |t| v2(60.0 + (v * t) as f32, yc), None);
            } else {
                // meia volta numa curva a ~3000 px/s
                let (cx, r) = (520.0f32, 30.0f32);
                let w = 3000.0 / r as f64;
                movimento(&mut img, &cfg, 0.09, move |t| {
                    let a = (w * t) as f32 - 1.2;
                    v2(cx + r * a.cos() * 3.0, yc + r * a.sin() * 0.9)
                }, None);
            }
        }
    }
    // Clique: o mesmo clique fotografado em 4 momentos.
    let y0 = altura_linha * casos.len() * 2;
    img.retangulo(0, y0, largura / 2, 140, BRANCO);
    img.retangulo(largura / 2, y0, largura / 2, 140, ESCURO);
    for (i, quando) in [0.04, 0.10, 0.18, 0.28].iter().enumerate() {
        for metade in 0..2 {
            let x = (metade * largura / 2 + 50 + i * 90) as f32;
            let y = (y0 + 60) as f32;
            movimento(&mut img, &cfg, 0.02 + quando, move |_| v2(x, y), Some(0.02));
        }
    }
    img.salvar(&pasta.join("movimento.png"), 1);
}

pub fn diagnostico(caminho: &Path) {
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let mut s = String::new();
    let _ = writeln!(s, "FlowCursor {} — diagnóstico", env!("CARGO_PKG_VERSION"));

    for (r, nome, esc) in sistema::listar_monitores() {
        let _ = writeln!(s, "monitor {nome}: {}x{} em ({}, {}), escala {:.0}%", r.right - r.left, r.bottom - r.top, r.left, r.top, esc * 100.0);
    }

    let mut vb = Vblank::abrir();
    vb.esperar();
    let ini = Instant::now();
    let n = 164;
    for _ in 0..n {
        vb.esperar();
    }
    let hz = n as f64 / ini.elapsed().as_secs_f64();
    let _ = writeln!(s, "sincronia: {} medindo {:.1} atualizações/s", vb.metodo, hz);

    let mapa = MapaCursores::novo();
    let _ = writeln!(s, "identificadores dos cursores do sistema:");
    for (h, t) in mapa.identificadores() {
        let _ = writeln!(s, "  {:?}: {:#x}", t, h);
    }
    let mut ci = CURSORINFO { cbSize: 24, flags: 0, hCursor: 0, ptScreenPos: POINT::default() };
    unsafe { GetCursorInfo(&mut ci) };
    let _ = writeln!(s, "cursor agora: {:#x} visível={} tipo={:?}", ci.hCursor, ci.flags & CURSOR_SHOWING != 0, mapa.tipo(ci.hCursor));

    // Troca rápida (uns 100 ms) para confirmar que esconder e devolver funcionam.
    let antes = sistema::seta_em_branco();
    let ok = sistema::esconder_cursores();
    std::thread::sleep(Duration::from_millis(50));
    let escondido = sistema::seta_em_branco();
    let mapa_depois = MapaCursores::novo();
    let mesmos = mapa.identificadores() == mapa_depois.identificadores();
    unsafe { GetCursorInfo(&mut ci) };
    let tipo_escondido = mapa.tipo(ci.hCursor);
    sistema::restaurar_cursores();
    std::thread::sleep(Duration::from_millis(50));
    let depois = sistema::seta_em_branco();
    let _ = writeln!(s, "seta em branco antes: {antes} (esperado false)");
    let _ = writeln!(s, "esconder: chamadas ok={ok}, seta em branco={escondido} (esperado true)");
    let _ = writeln!(s, "identificadores iguais depois de esconder: {mesmos} (esperado true); tipo lido escondido: {tipo_escondido:?}");
    let _ = writeln!(s, "seta em branco depois de devolver: {depois} (esperado false)");

    let fg = unsafe { GetForegroundWindow() };
    let _ = writeln!(s, "janela em primeiro plano: {} ({})", sistema::processo(fg), sistema::classe(fg));

    // Desempenho do motor num percurso em "oito" com velocidade variando até ~6000 px/s.
    let cfg = Config::default();
    let mut m = Motor::novo();
    let mut tempos = Vec::new();
    let mut t = 0.0f64;
    for i in 0..3000 {
        let fase = t * 2.2;
        let vel = 0.5 + 0.5 * (t * 0.7).sin();
        let p = v2(1280.0 + (900.0 * fase.sin() * vel) as f32, 720.0 + (400.0 * (2.0 * fase).sin() * vel) as f32);
        let tipo = if (i / 400) % 2 == 0 { Tipo::Seta } else { Tipo::Mao };
        let c = Instant::now();
        let _ = m.quadro(t, p, tipo, i % 300 < 20, 1.0, &cfg).map(|q| q.w * q.h);
        tempos.push(c.elapsed().as_secs_f64() * 1000.0);
        t += DT;
    }
    tempos.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let media = tempos.iter().sum::<f64>() / tempos.len() as f64;
    let _ = writeln!(
        s,
        "motor (3000 quadros simulados): média {:.3} ms, 95% até {:.3} ms, pior {:.3} ms (orçamento a 164 Hz: 6,1 ms)",
        media,
        tempos[tempos.len() * 95 / 100],
        tempos[tempos.len() - 1]
    );
    let _ = std::fs::write(caminho, s);
}
