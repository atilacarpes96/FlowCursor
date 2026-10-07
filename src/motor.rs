// Motor do ponteiro: física de mola, borrão de movimento, rastro e onda do clique.
// Não depende de janelas; recebe a posição real e devolve a imagem do quadro.
use crate::config::{Config, Estilo};
use crate::formas::{paleta, premul, smoothstep, sobre, v2, Camada, Cor, Sprite, Tipo, V2};
use std::collections::VecDeque;
use std::f32::consts::TAU;

const DURACAO_ONDA: f64 = 0.38;
const LADO_MAXIMO: usize = 1600;
/// O rastro é cortado a esta distância da cabeça, para a janela não crescer demais.
const ALCANCE_RASTRO: f32 = 500.0;
const MAX_COPIAS_RASTRO: usize = 220;

#[derive(Clone, Copy)]
struct Mola {
    x: f32,
    v: f32,
}

impl Mola {
    fn passo(&mut self, alvo: f32, dt: f32, freq: f32, amortecimento: f32) {
        let w = TAU * freq;
        let n = (dt / 0.001).ceil().max(1.0) as usize;
        let h = dt / n as f32;
        for _ in 0..n {
            let a = (alvo - self.x) * w * w - self.v * 2.0 * amortecimento * w;
            self.v += a * h;
            self.x += self.v * h;
        }
    }
    fn parada(&self, alvo: f32) -> bool {
        (self.x - alvo).abs() < 1e-3 && self.v.abs() < 1e-2
    }
}

struct PontoRastro {
    t: f64,
    /// Ponto ativo do ponteiro desenhado naquele quadro.
    p: V2,
    vel: f32,
}

#[derive(Clone, PartialEq)]
struct ChaveSprite {
    tipo: Tipo,
    anterior: Option<Tipo>,
    transicao: u16,
    escala: u32,
    estilo: Estilo,
    sombra: u16,
    fase: u16,
}

pub struct Quadro<'a> {
    pub x: i32,
    pub y: i32,
    pub w: usize,
    pub h: usize,
    /// BGRA pré-multiplicado, linha a linha, largura `w`.
    pub px: &'a [u32],
}

pub struct Motor {
    pos: V2,
    vel: V2,
    alvo_ant: V2,
    v_alvo: V2,
    iniciado: bool,
    t_ant: f64,
    escala: Mola,
    tipo: Tipo,
    tipo_ant: Option<Tipo>,
    transicao: f32,
    pressionado_ant: bool,
    rastro: VecDeque<PontoRastro>,
    ondas: Vec<(f64, V2)>,
    sprite: Sprite,
    /// A mesma forma sem sombra: as cópias do rastro com sombra viravam uma mancha cinza.
    sprite_rastro: Sprite,
    chave_sprite: Option<ChaveSprite>,
    versao_sprite: u64,
    ultimo: Option<(i64, i64, u64)>,
    forcar: bool,
    camada: Vec<Cor>,
    acum: Vec<Cor>,
    saida: Vec<u32>,
}

impl Motor {
    pub fn novo() -> Self {
        Motor {
            pos: V2::default(),
            vel: V2::default(),
            alvo_ant: V2::default(),
            v_alvo: V2::default(),
            iniciado: false,
            t_ant: 0.0,
            escala: Mola { x: 1.0, v: 0.0 },
            tipo: Tipo::Seta,
            tipo_ant: None,
            transicao: 1.0,
            pressionado_ant: false,
            rastro: VecDeque::new(),
            ondas: Vec::new(),
            sprite: Sprite::novo(),
            sprite_rastro: Sprite::novo(),
            chave_sprite: None,
            versao_sprite: 0,
            ultimo: None,
            forcar: true,
            camada: Vec::new(),
            acum: Vec::new(),
            saida: Vec::new(),
        }
    }

    /// Esquece o movimento: o próximo quadro começa parado na posição real.
    pub fn resetar(&mut self) {
        self.iniciado = false;
        self.rastro.clear();
        self.ondas.clear();
        self.forcar = true;
    }

    pub fn velocidade(&self) -> f32 {
        self.vel.len()
    }

    pub fn atraso(&self) -> f32 {
        (self.alvo_ant - self.pos).len()
    }

    fn fisica(&mut self, dt: f32, alvo: V2, cfg: &Config) {
        let v_inst = (alvo - self.alvo_ant) / dt;
        self.v_alvo = self.v_alvo.lerp(v_inst, 0.5);
        if cfg.responsividade >= 99.5 {
            self.vel = self.v_alvo;
            self.pos = alvo;
            self.alvo_ant = alvo;
            return;
        }
        // Responsividade 0..100 vira frequência da mola de 4 a 60 Hz (escala exponencial).
        let w = TAU * 4.0 * 15f32.powf(cfg.responsividade / 100.0);
        let z = 1.0 - 0.65 * cfg.elasticidade / 100.0;
        let k = cfg.antecipacao / 100.0;
        let n = (dt / 0.001).ceil().max(1.0) as usize;
        let h = dt / n as f32;
        for i in 0..n {
            // o alvo anda em linha reta durante o quadro, em vez de saltar no começo
            let a_i = self.alvo_ant.lerp(alvo, (i + 1) as f32 / n as f32);
            let acc = (a_i - self.pos) * (w * w) + (self.v_alvo * k - self.vel) * (2.0 * z * w);
            self.vel += acc * h;
            self.pos += self.vel * h;
        }
        self.alvo_ant = alvo;
        if (alvo - self.pos).len() < 0.05 && self.vel.len() < 3.0 && self.v_alvo.len() < 3.0 {
            self.pos = alvo;
            self.vel = V2::default();
        }
    }

    /// Avança a simulação até `t` (segundos) e devolve a imagem do quadro,
    /// ou None quando nada mudou desde o último quadro devolvido.
    pub fn quadro(&mut self, t: f64, alvo: V2, tipo: Tipo, pressionado: bool, dpi: f32, cfg: &Config) -> Option<Quadro<'_>> {
        let escala_base = cfg.tamanho * dpi;
        if !self.iniciado || (alvo - self.pos).len() > 3000.0 {
            self.pos = alvo;
            self.vel = V2::default();
            self.alvo_ant = alvo;
            self.v_alvo = V2::default();
            self.t_ant = t;
            self.tipo = tipo;
            self.tipo_ant = None;
            self.rastro.clear();
            self.iniciado = true;
            self.forcar = true;
        }
        let dt = ((t - self.t_ant) as f32).clamp(0.0, 0.05);
        self.t_ant = t;
        let pos_ant = self.pos;
        if dt > 0.0 {
            self.fisica(dt, alvo, cfg);
        }

        // Troca de forma: mistura as duas por 110 ms com um pequeno "pulo".
        if tipo != self.tipo {
            self.tipo_ant = Some(self.tipo);
            self.tipo = tipo;
            self.transicao = 0.0;
            self.escala.x = self.escala.x.min(1.0) * 0.9;
        }
        if self.tipo_ant.is_some() {
            self.transicao += dt / 0.11;
            if self.transicao >= 1.0 {
                self.tipo_ant = None;
                self.transicao = 1.0;
            }
        }
        let alvo_escala = if pressionado && cfg.clique { 0.86 } else { 1.0 };
        if dt > 0.0 {
            self.escala.passo(alvo_escala, dt, 9.0, 0.45);
        }

        if pressionado && !self.pressionado_ant && cfg.onda_clique {
            self.ondas.push((t, alvo));
        }
        self.pressionado_ant = pressionado;
        self.ondas.retain(|(t0, _)| t - t0 < DURACAO_ONDA);

        let janela_rastro = (cfg.rastro / 100.0 * 0.14) as f64;
        if janela_rastro > 0.0 && cfg.rastro_intensidade > 0.0 {
            self.rastro.push_back(PontoRastro { t, p: self.pos, vel: self.vel.len() });
            while self.rastro.front().is_some_and(|f| t - f.t > janela_rastro) {
                self.rastro.pop_front();
            }
        } else {
            self.rastro.clear();
        }

        // Sprites: só redesenha quando forma, escala, estilo ou animação mudam.
        let escala_total = escala_base * self.escala.x;
        let animado = self.tipo.animado() || self.tipo_ant.is_some_and(|x| x.animado());
        let fase = if animado { ((t * 1.2).fract() * 720.0) as u16 } else { 0 };
        let chave = ChaveSprite {
            tipo: self.tipo,
            anterior: self.tipo_ant,
            transicao: (self.transicao * 1000.0) as u16,
            escala: (escala_total * 2000.0) as u32,
            estilo: cfg.estilo,
            sombra: cfg.sombra as u16,
            fase,
        };
        if self.chave_sprite.as_ref() != Some(&chave) {
            let mut camadas = Vec::with_capacity(2);
            if let Some(ant) = self.tipo_ant {
                let e = smoothstep(0.0, 1.0, self.transicao);
                camadas.push(Camada { tipo: ant, alfa: 1.0 - e, escala: escala_total });
                camadas.push(Camada { tipo: self.tipo, alfa: e, escala: escala_total });
            } else {
                camadas.push(Camada { tipo: self.tipo, alfa: 1.0, escala: escala_total });
            }
            let tempo = (t % 1000.0) as f32;
            self.sprite.renderizar(&camadas, cfg.estilo, cfg.sombra, tempo);
            self.sprite_rastro.renderizar(&camadas, cfg.estilo, 0.0, tempo);
            self.chave_sprite = Some(chave);
            self.versao_sprite += 1;
        }

        let v0 = cfg.rastro_velocidade;
        let rastro_visivel = self.rastro.len() > 1
            && self.rastro.iter().any(|p| p.vel > v0)
            && (self.rastro.front().unwrap().p - self.rastro.back().unwrap().p).len() > 0.3;
        let animando = self.vel.len() > 0.01
            || (pos_ant - self.pos).len() > 0.001
            || rastro_visivel
            || !self.ondas.is_empty()
            || !self.escala.parada(alvo_escala);
        let assinatura = ((self.pos.x * 64.0) as i64, (self.pos.y * 64.0) as i64, self.versao_sprite);
        if !animando && !self.forcar && self.ultimo == Some(assinatura) {
            return None;
        }
        self.ultimo = Some(assinatura);
        self.forcar = false;

        Some(self.compor(t, pos_ant, escala_base, rastro_visivel, cfg))
    }

    fn compor(&mut self, t: f64, pos_ant: V2, escala_base: f32, rastro_visivel: bool, cfg: &Config) -> Quadro<'_> {
        // Trajeto do borrão: a fração do último quadro em que o "obturador" fica aberto.
        let desloc = (self.pos - pos_ant) * (cfg.borrao / 100.0);
        let comp_total = desloc.len();
        let dir = if comp_total > 1e-4 { desloc / comp_total } else { V2::default() };
        let comp = comp_total.min(400.0);
        let cauda = self.pos - dir * comp;

        // Caixa que contém tudo o que será desenhado neste quadro.
        let sp = &self.sprite;
        let (esq, topo) = (-(sp.ox as f32) - 2.0, -(sp.oy as f32) - 2.0);
        let (dir_x, baixo) = (sp.w as f32 - sp.ox as f32 + 2.0, sp.h as f32 - sp.oy as f32 + 2.0);
        let mut min = v2(self.pos.x.min(cauda.x) + esq, self.pos.y.min(cauda.y) + topo);
        let mut max = v2(self.pos.x.max(cauda.x) + dir_x, self.pos.y.max(cauda.y) + baixo);
        if rastro_visivel {
            for pr in self.rastro.iter().filter(|pr| (pr.p - self.pos).len() <= ALCANCE_RASTRO) {
                min = v2(min.x.min(pr.p.x + esq), min.y.min(pr.p.y + topo));
                max = v2(max.x.max(pr.p.x + dir_x), max.y.max(pr.p.y + baixo));
            }
        }
        let raio_onda = 21.0 * escala_base;
        for (_, c) in &self.ondas {
            min = v2(min.x.min(c.x - raio_onda), min.y.min(c.y - raio_onda));
            max = v2(max.x.max(c.x + raio_onda), max.y.max(c.y + raio_onda));
        }
        let ox = min.x.floor() as i32;
        let oy = min.y.floor() as i32;
        let w = ((max.x.ceil() as i32 - ox).max(1) as usize).min(LADO_MAXIMO);
        let h = ((max.y.ceil() as i32 - oy).max(1) as usize).min(LADO_MAXIMO);
        let origem = v2(ox as f32, oy as f32);
        let n_px = w * h;
        for buf in [&mut self.camada, &mut self.acum] {
            if buf.len() < n_px {
                buf.resize(n_px, [0.0; 4]);
            }
            buf[..n_px].fill([0.0; 4]);
        }

        if rastro_visivel {
            self.desenhar_rastro(t, origem, w, h, escala_base, comp, cfg);
        }
        if !self.ondas.is_empty() {
            let pal = paleta(cfg.estilo);
            self.desenhar_ondas(t, origem, w, h, escala_base, &pal.preenchimento, &pal.contorno);
        }

        // Cabeça: várias cópias ao longo do trajeto do quadro. A frente pesa bem mais,
        // para a forma continuar legível mesmo em movimento rápido.
        let n = if comp < 0.75 { 1 } else { (comp / 0.9).ceil().min(64.0) as usize };
        let pesos: Vec<f32> = (0..n)
            .map(|k| {
                let s = k as f32 / (n.max(2) - 1) as f32;
                0.1 + 0.9 * (1.0 - s) * (1.0 - s)
            })
            .collect();
        let soma: f32 = pesos.iter().sum();
        for (k, peso) in pesos.iter().enumerate() {
            let s = if n == 1 { 0.0 } else { k as f32 / (n - 1) as f32 };
            let q = self.pos - dir * (comp * s) - origem;
            espalhar(&self.sprite, q, peso / soma, &mut self.acum, w, h);
        }

        if self.saida.len() < n_px {
            self.saida.resize(n_px, 0);
        }
        for i in 0..n_px {
            let c = sobre(self.acum[i], self.camada[i]);
            let a = c[3].clamp(0.0, 1.0);
            let canal = |x: f32| (x.clamp(0.0, a) * 255.0 + 0.5) as u32;
            self.saida[i] = canal(a) << 24 | canal(c[0]) << 16 | canal(c[1]) << 8 | canal(c[2]);
        }
        Quadro { x: ox, y: oy, w, h, px: &self.saida[..n_px] }
    }

    /// Rastro em "longa exposição": cópias da própria forma ao longo do caminho
    /// recente, cada vez mais transparentes. Começa onde termina o borrão da cabeça.
    #[allow(clippy::too_many_arguments)]
    fn desenhar_rastro(&mut self, t: f64, origem: V2, w: usize, h: usize, esc: f32, comp_cabeca: f32, cfg: &Config) {
        let janela = (cfg.rastro / 100.0 * 0.14) as f64;
        let v0 = cfg.rastro_velocidade;
        let intensidade = cfg.rastro_intensidade / 100.0;
        // (posição na janela, opacidade desejada do rastro naquele ponto), da cabeça para a cauda
        let brutos: Vec<(V2, f32)> = self
            .rastro
            .iter()
            .rev()
            .filter(|pr| (pr.p - self.pos).len() <= ALCANCE_RASTRO)
            .map(|pr| {
                let u = ((t - pr.t) / janela).clamp(0.0, 1.0) as f32;
                let fator = smoothstep(v0, v0 * 2.2, pr.vel);
                (pr.p - origem, intensidade * (1.0 - u).powf(1.6) * fator)
            })
            .collect();
        if brutos.len() < 2 {
            return;
        }
        // Os pontos são um por quadro; em curvas fechadas o caminho ficaria com quinas.
        // Catmull-Rom passa uma curva suave por eles.
        let mut caminho: Vec<(V2, f32)> = Vec::with_capacity(brutos.len() * 6);
        for i in 0..brutos.len() - 1 {
            let p0 = brutos[i.saturating_sub(1)].0;
            let (p1, o1) = brutos[i];
            let (p2, o2) = brutos[i + 1];
            let p3 = brutos[(i + 2).min(brutos.len() - 1)].0;
            let partes = (((p2 - p1).len() / 2.0).ceil() as usize).clamp(1, 16);
            for k in 0..partes {
                let s = k as f32 / partes as f32;
                let (s2, s3) = (s * s, s * s * s);
                let p = (p1 * 2.0 + (p2 - p0) * s + (p0 * 2.0 - p1 * 5.0 + p2 * 4.0 - p3) * s2 + (p1 * 3.0 - p0 - p2 * 3.0 + p3) * s3) * 0.5;
                caminho.push((p, o1 + (o2 - o1) * s));
            }
        }
        caminho.push(*brutos.last().unwrap());

        // Amostras a intervalos iguais ao longo do caminho.
        let total: f32 = caminho.windows(2).map(|s| (s[1].0 - s[0].0).len()).sum();
        if total <= comp_cabeca + 1.0 {
            return;
        }
        let passo = ((total - comp_cabeca) / MAX_COPIAS_RASTRO as f32).max(1.25);
        let mut amostras: Vec<(V2, f32)> = Vec::new();
        let mut proxima = comp_cabeca;
        let mut andado = 0.0;
        for s in caminho.windows(2) {
            let (a, oa) = s[0];
            let (b, ob) = s[1];
            let l = (b - a).len();
            while l > 0.0 && proxima <= andado + l {
                let f = (proxima - andado) / l;
                amostras.push((a.lerp(b, f), oa + (ob - oa) * f));
                proxima += passo;
            }
            andado += l;
        }

        // Cada pixel é coberto por várias cópias seguidas (a forma tem ~12 px no sentido
        // do movimento); a opacidade de cada cópia compensa isso.
        let extensao = 12.0 * esc;
        for &(p, opac) in amostras.iter().rev() {
            if opac < 0.003 {
                continue;
            }
            let alfa = 1.0 - (1.0 - opac.min(0.95)).powf(passo / extensao);
            sobrepor(&self.sprite_rastro, p, alfa, &mut self.camada, w, h);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn desenhar_ondas(&mut self, t: f64, origem: V2, w: usize, h: usize, esc: f32, preench: &Cor, contorno: &Cor) {
        for &(t0, centro) in &self.ondas {
            let u = ((t - t0) / DURACAO_ONDA).clamp(0.0, 1.0) as f32;
            let e = 1.0 - (1.0 - u).powi(3);
            let raio = (3.0 + 15.0 * e) * esc;
            let meia = (1.5 * (1.0 - u) + 0.5) * esc;
            let alfa = 0.65 * (1.0 - u) * (1.0 - u);
            let c = centro - origem;
            let m = raio + meia + 2.0 * esc;
            let x0 = (c.x - m).floor().max(0.0) as usize;
            let y0 = (c.y - m).floor().max(0.0) as usize;
            let x1 = ((c.x + m).ceil().max(0.0) as usize).min(w);
            let y1 = ((c.y + m).ceil().max(0.0) as usize).min(h);
            for y in y0..y1 {
                for x in x0..x1 {
                    let d = ((v2(x as f32 + 0.5, y as f32 + 0.5) - c).len() - raio).abs() - meia;
                    let miolo = (0.5 - d).clamp(0.0, 1.0) * alfa;
                    let borda = (0.5 - (d - 0.9 * esc)).clamp(0.0, 1.0) * alfa * 0.8;
                    if borda <= 0.0 {
                        continue;
                    }
                    // anel na cor do contorno com borda na cor do preenchimento: aparece em fundo claro e escuro
                    let cor = sobre(premul(*contorno, miolo), premul(*preench, borda));
                    let i = y * w + x;
                    self.camada[i] = sobre(cor, self.camada[i]);
                }
            }
        }
    }
}

/// Pesos de um ponto com fração de pixel entre os 4 vizinhos.
fn vizinhos(sp: &Sprite, q: V2, peso: f32) -> (i32, i32, [(i32, i32, f32); 4]) {
    let bx = q.x - sp.ox as f32;
    let by = q.y - sp.oy as f32;
    let (fx0, fy0) = (bx.floor(), by.floor());
    let (fx, fy) = (bx - fx0, by - fy0);
    (
        fx0 as i32,
        fy0 as i32,
        [
            (0, 0, (1.0 - fx) * (1.0 - fy) * peso),
            (1, 0, fx * (1.0 - fy) * peso),
            (0, 1, (1.0 - fx) * fy * peso),
            (1, 1, fx * fy * peso),
        ],
    )
}

/// Soma o sprite no destino na posição `q` (ponto ativo, com fração de pixel).
/// Usado no borrão, em que as cópias se somam até a opacidade total.
fn espalhar(sp: &Sprite, q: V2, peso: f32, dest: &mut [Cor], w: usize, h: usize) {
    let (ix, iy, pesos) = vizinhos(sp, q, peso);
    for &idx in &sp.ativos {
        let idx = idx as usize;
        let (i, j) = ((idx % sp.w) as i32, (idx / sp.w) as i32);
        let c = sp.px[idx];
        for &(dx, dy, p) in &pesos {
            let (x, y) = (ix + i + dx, iy + j + dy);
            if p <= 0.0 || x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                continue;
            }
            let d = &mut dest[y as usize * w + x as usize];
            d[0] += c[0] * p;
            d[1] += c[1] * p;
            d[2] += c[2] * p;
            d[3] += c[3] * p;
        }
    }
}

/// Desenha o sprite por cima do destino com opacidade `alfa` (usado no rastro).
fn sobrepor(sp: &Sprite, q: V2, alfa: f32, dest: &mut [Cor], w: usize, h: usize) {
    let (ix, iy, pesos) = vizinhos(sp, q, alfa);
    for &idx in &sp.ativos {
        let idx = idx as usize;
        let (i, j) = ((idx % sp.w) as i32, (idx / sp.w) as i32);
        let c = sp.px[idx];
        for &(dx, dy, p) in &pesos {
            let (x, y) = (ix + i + dx, iy + j + dy);
            if p <= 0.0 || x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                continue;
            }
            let d = &mut dest[y as usize * w + x as usize];
            *d = sobre([c[0] * p, c[1] * p, c[2] * p, c[3] * p], *d);
        }
    }
}
