// Motor do ponteiro: física de mola, borrão de movimento, rastro e onda do clique.
// Não depende de janelas; recebe a posição real e devolve a imagem do quadro.
use crate::config::Config;
use crate::formas::{premul, smoothstep, sobre, v2, Camada, Cor, Sprite, Tipo, V2};
use crate::temas::{self, Tema};
use std::collections::VecDeque;
use std::f32::consts::TAU;
use std::sync::Arc;

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
    tema: u64,
    sombra: u16,
    fase: u32,
    ima: u16,
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
    tema: Option<Arc<Tema>>,
    /// Último fundo usado no vidro (para redesenhar quando a tela atrás muda).
    fundo_ant: usize,
    /// Ímã: 0 = ponteiro normal, 1 = virou a pílula grudada no botão.
    ima: f32,
    /// Pílula (x, y, largura, altura) na tela; anda com mola até o botão e de volta.
    caixa: [f32; 4],
    pilula: Sprite,
    chave_pilula: (i32, i32, u64, u16),
    /// Onde o mouse está de verdade (sem a mola), para marcar o ponto dentro da pílula.
    mouse: V2,
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
            tema: None,
            fundo_ant: 0,
            ima: 0.0,
            caixa: [0.0; 4],
            pilula: Sprite::novo(),
            chave_pilula: (0, 0, 0, 0),
            mouse: V2::default(),
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
        self.mouse = alvo;
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

        // Sprites: só redesenha quando forma, escala, tema ou animação mudam.
        let tema = temas::obter(cfg);

        // Ímã: sobre um botão, o ponteiro de vidro se desfaz numa pílula do tamanho dele,
        // que acompanha um pouco o mouse; fora, a pílula volta a ser o ponteiro.
        let botao = if tema.usa_fundo() && cfg.grudar_botoes { crate::alvo::sob(alvo.x as i32, alvo.y as i32) } else { None };
        let lente = tema.vetor.forma == crate::formas::Forma::Lente;
        // na Lente, a pílula para no tamanho de 100%
        let s = escala_base * fator_pilula(lente, cfg.tamanho);
        let s_seta = escala_base * fator_lente(lente, Tipo::Seta, cfg.tamanho);
        let alvo_caixa = match botao {
            Some(r) => {
                let (cx, cy) = ((r.left + r.right) as f32 / 2.0, (r.top + r.bottom) as f32 / 2.0);
                [r.left as f32 - 4.0 * s + (alvo.x - cx) * 0.06, r.top as f32 - 4.0 * s + (alvo.y - cy) * 0.1, (r.right - r.left) as f32 + 8.0 * s, (r.bottom - r.top) as f32 + 8.0 * s]
            }
            None => [self.pos.x - 2.0 * s_seta, self.pos.y - 2.0 * s_seta, 16.0 * s_seta, 22.0 * s_seta],
        };
        let alvo_ima = if botao.is_some() { 1.0 } else { 0.0 };
        let ima_ant = self.ima;
        let caixa_ant = self.caixa;
        if self.ima < 0.01 && botao.is_none() {
            self.caixa = alvo_caixa;
        } else if dt > 0.0 {
            let f = (dt * 14.0).min(1.0);
            for k in 0..4 {
                self.caixa[k] += (alvo_caixa[k] - self.caixa[k]) * f;
            }
        }
        if dt > 0.0 {
            self.ima += (alvo_ima - self.ima) * (dt * 12.0).min(1.0);
            if (self.ima - alvo_ima).abs() < 0.003 {
                self.ima = alvo_ima;
            }
        }
        let mexeu_ima = self.ima != ima_ant || (self.ima > 0.0 && caixa_ant.iter().zip(&self.caixa).any(|(a, b)| (a - b).abs() > 0.05));

        let escala_total = escala_base * self.escala.x * (1.0 - 0.6 * self.ima);
        let tempo = (t % 1000.0) as f32;
        let fase = tema.fase(self.tipo, tempo) ^ self.tipo_ant.map_or(0, |a| tema.fase(a, tempo) << 16);
        let chave = ChaveSprite {
            tipo: self.tipo,
            anterior: self.tipo_ant,
            transicao: (self.transicao * 1000.0) as u16,
            escala: (escala_total * 2000.0) as u32,
            tema: tema.serie,
            sombra: cfg.sombra as u16,
            fase,
            ima: (self.ima * 1000.0) as u16,
        };
        if self.chave_sprite.as_ref() != Some(&chave) {
            let mut camadas = Vec::with_capacity(2);
            let visivel = 1.0 - self.ima;
            if let Some(ant) = self.tipo_ant {
                let e = smoothstep(0.0, 1.0, self.transicao);
                camadas.push(Camada { tipo: ant, alfa: (1.0 - e) * visivel, escala: escala_total * fator_lente(lente, ant, cfg.tamanho) });
                camadas.push(Camada { tipo: self.tipo, alfa: e * visivel, escala: escala_total * fator_lente(lente, self.tipo, cfg.tamanho) });
            } else {
                camadas.push(Camada { tipo: self.tipo, alfa: visivel, escala: escala_total * fator_lente(lente, self.tipo, cfg.tamanho) });
            }
            self.sprite.renderizar(&camadas, &tema, cfg.sombra, tempo);
            self.sprite_rastro.renderizar(&camadas, &tema, 0.0, tempo);
            self.chave_sprite = Some(chave);
            self.versao_sprite += 1;
        }
        if self.ima > 0.01 {
            let chave = (self.caixa[2].round() as i32, self.caixa[3].round() as i32, tema.serie, cfg.sombra as u16);
            if chave != self.chave_pilula {
                self.pilula.renderizar_pilula(chave.0 as f32, chave.1 as f32, s, &tema, cfg.sombra);
                self.chave_pilula = chave;
            }
        }
        self.tema = Some(tema);

        let v0 = cfg.rastro_velocidade;
        let rastro_visivel = self.rastro.len() > 1
            && self.rastro.iter().any(|p| p.vel > v0)
            && (self.rastro.front().unwrap().p - self.rastro.back().unwrap().p).len() > 0.3;
        let animando = self.vel.len() > 0.01
            || (pos_ant - self.pos).len() > 0.001
            || rastro_visivel
            || !self.ondas.is_empty()
            || !self.escala.parada(alvo_escala)
            || mexeu_ima;
        // com vidro, a tela atrás pode mudar com o ponteiro parado
        let fundo = if self.sprite.vidro.is_empty() { None } else { crate::captura::atual() };
        let id_fundo = fundo.as_ref().map_or(0, |f| Arc::as_ptr(f) as usize);
        let animando = animando || id_fundo != self.fundo_ant;
        self.fundo_ant = id_fundo;
        let assinatura = ((self.pos.x * 64.0) as i64, (self.pos.y * 64.0) as i64, self.versao_sprite);
        if !animando && !self.forcar && self.ultimo == Some(assinatura) {
            return None;
        }
        self.ultimo = Some(assinatura);
        self.forcar = false;

        Some(self.compor(t, pos_ant, escala_base, rastro_visivel, cfg, fundo.as_deref()))
    }

    #[allow(clippy::too_many_arguments)]
    fn compor(&mut self, t: f64, pos_ant: V2, escala_base: f32, rastro_visivel: bool, cfg: &Config, fundo: Option<&crate::captura::Fundo>) -> Quadro<'_> {
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
        if self.ima > 0.01 {
            let (p, c) = (&self.pilula, self.caixa);
            min = v2(min.x.min(c[0] - p.ox as f32 - 1.0), min.y.min(c[1] - p.oy as f32 - 1.0));
            max = v2(max.x.max(c[0] - p.ox as f32 + p.w as f32 + 1.0), max.y.max(c[1] - p.oy as f32 + p.h as f32 + 1.0));
        }
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
            let pal = self.tema.as_ref().map_or(crate::formas::MACOS.pal, |t| t.paleta());
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

        // Vidro: a tela atrás por baixo da forma da frente (Cristal: meio desfocada, mais na
        // borda; Lente: nítida, quase sem ampliar no meio e entortando na borda).
        let lente = self.tema.as_ref().is_some_and(|t| t.vetor.forma == crate::formas::Forma::Lente);
        if let Some(f) = fundo {
            let q = self.pos - origem;
            let (ix, iy) = ((q.x - self.sprite.ox as f32).round() as i32, (q.y - self.sprite.oy as f32).round() as i32);
            vidro_por_baixo(&self.sprite, ix, iy, (ox, oy, w, h), f, lente, 1.12, &mut self.acum);
        }

        // Pílula do ímã, por baixo do ponteiro (que some enquanto ela aparece).
        if self.ima > 0.01 {
            let p = &self.pilula;
            let (ix, iy) = ((self.caixa[0] - origem.x).round() as i32 - p.ox, (self.caixa[1] - origem.y).round() as i32 - p.oy);
            let mut camada_pilula = vec![[0.0f32; 4]; p.w * p.h];
            for (i, c) in camada_pilula.iter_mut().enumerate() {
                *c = p.px[i];
            }
            if let Some(f) = fundo {
                vidro_por_baixo(p, 0, 0, (ox + ix, oy + iy, p.w, p.h), f, lente, 1.03, &mut camada_pilula);
            }
            // a pílula ocupa o botão inteiro: o brilho e o ponto mostram onde o mouse está
            let s = escala_base * fator_pilula(lente, cfg.tamanho);
            marcar_mouse(&mut camada_pilula, p.w, p.h, self.mouse.x - (ox + ix) as f32, self.mouse.y - (oy + iy) as f32, s);
            for j in 0..p.h {
                for i in 0..p.w {
                    let (x, y) = (ix + i as i32, iy + j as i32);
                    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                        continue;
                    }
                    let c = camada_pilula[j * p.w + i].map(|v| v * self.ima);
                    let d = &mut self.camada[y as usize * w + x as usize];
                    *d = sobre(c, *d);
                }
            }
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

/// Lente líquida: todas as formas são desenhadas na escala 1,85x (o 100% é o antigo 185%,
/// que era o tamanho usável), para o vidro ter a mesma espessura nelas. Só a seta cresce
/// acima de 100%; as outras param no tamanho de 100%.
pub const LENTE_SETA: f32 = 1.85;
pub fn fator_lente(lente: bool, tipo: Tipo, tamanho: f32) -> f32 {
    if !lente {
        1.0
    } else if matches!(tipo, Tipo::Seta | Tipo::SetaEspera) {
        LENTE_SETA
    } else {
        LENTE_SETA * fator_pilula(lente, tamanho)
    }
}

/// A pílula dos botões acompanha o botão e também para no tamanho de 100%.
fn fator_pilula(lente: bool, tamanho: f32) -> f32 {
    if lente { tamanho.min(1.0) / tamanho.max(0.01) } else { 1.0 }
}

/// Marca na pílula (`w` x `h`, já com a tela de trás) onde o mouse está: uma mancha de luz
/// difusa, sem contorno, para a pílula continuar sendo o ponteiro. Em fundo claro a luz branca
/// não apareceria, então ali ela vira uma sombra suave. Só pinta onde já há pílula.
fn marcar_mouse(px: &mut [Cor], w: usize, h: usize, mx: f32, my: f32, s: f32) {
    let (brilho, miolo) = (26.0 * s, 8.0 * s);
    let (x0, x1) = (((mx - brilho).floor().max(0.0)) as usize, ((mx + brilho).ceil().max(0.0) as usize).min(w));
    let (y0, y1) = (((my - brilho).floor().max(0.0)) as usize, ((my + brilho).ceil().max(0.0) as usize).min(h));
    // a claridade média em volta do mouse decide: um valor só, para o texto do botão não manchar
    let (mut soma, mut n) = (0.0, 0.0);
    for j in y0..y1 {
        for i in x0..x1 {
            let c = px[j * w + i];
            if c[3] > 0.5 {
                soma += (0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]) / c[3];
                n += 1.0;
            }
        }
    }
    let claro = smoothstep(0.55, 0.85, if n > 0.0 { soma / n } else { 0.0 });
    let (cor, forca) = (1.0 - 0.45 * claro, 1.0 - 0.45 * claro);
    for j in y0..y1 {
        for i in x0..x1 {
            let d = (v2(i as f32 + 0.5, j as f32 + 0.5) - v2(mx, my)).len();
            let c = &mut px[j * w + i];
            let a = c[3].min(1.0);
            if d >= brilho || a <= 0.0 {
                continue;
            }
            let g = (0.6 * (-(d / miolo).powi(2)).exp() + 0.16 * (1.0 - smoothstep(0.0, brilho, d))) * forca;
            for v in c.iter_mut().take(3) {
                *v += (cor * a - *v) * g;
            }
        }
    }
}

/// Põe a tela de trás por baixo do vidro do sprite, que está em (ix, iy) no destino.
/// `area` é (x, y, largura, altura) do destino na tela.
#[allow(clippy::too_many_arguments)]
fn vidro_por_baixo(sp: &Sprite, ix: i32, iy: i32, area: (i32, i32, usize, usize), f: &crate::captura::Fundo, lente: bool, zoom: f32, dest: &mut [Cor]) {
    let (ox, oy, w, h) = area;
    let centro = v2(ox as f32 + ix as f32 + sp.centro.x, oy as f32 + iy as f32 + sp.centro.y);
    for j in 0..sp.h {
        for i in 0..sp.w {
            let [cob, borda] = sp.vidro[j * sp.w + i];
            let (x, y) = (ix + i as i32, iy + j as i32);
            if cob <= 0.0 || x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                continue;
            }
            let c = if lente {
                // Como a Liquid Glass: o meio quase não amplia (fica nítido) e a borda
                // entorta a imagem, como vidro grosso. A amostra é bicúbica, sem degraus.
                let p = v2((ox + x) as f32 + 0.5, (oy + y) as f32 + 0.5);
                let z = zoom * (1.0 + 0.45 * borda * borda);
                let o = centro + (p - centro) / z;
                let Some(n) = f.nitido_suave(o.x, o.y) else { continue };
                n
            } else {
                let Some((n, b)) = f.em(ox + x, oy + y) else { continue };
                let k = (0.55 + 0.45 * borda).min(1.0);
                [n[0] + (b[0] - n[0]) * k, n[1] + (b[1] - n[1]) * k, n[2] + (b[2] - n[2]) * k]
            };
            let d = &mut dest[y as usize * w + x as usize];
            *d = sobre(*d, [c[0] * cob, c[1] * cob, c[2] * cob, cob.min(1.0)]);
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

#[cfg(test)]
mod testes {
    use super::*;

    /// Fundo escuro clareia no mouse, fundo claro escurece, e nada pinta fora da pílula.
    #[test]
    fn marca_o_mouse_so_dentro_da_pilula() {
        let (w, h) = (80, 40);
        let mut px = vec![[0.2, 0.2, 0.2, 1.0]; w * h];
        px[0] = [0.0; 4];
        marcar_mouse(&mut px, w, h, 40.5, 20.5, 1.0);
        assert!(px[20 * w + 40][0] > px[20 * w + 50][0] && px[20 * w + 50][0] > 0.2, "luz cai aos poucos");
        assert_eq!(px[0], [0.0; 4], "fora da pílula continua vazio");
        let mut px = vec![[0.95, 0.95, 0.95, 1.0]; w * h];
        marcar_mouse(&mut px, w, h, 40.5, 20.5, 1.0);
        assert!(px[20 * w + 40][0] < 0.9, "em fundo claro vira sombra");
        // mouse fora da imagem não pode estourar os índices
        marcar_mouse(&mut px, w, h, -500.0, 900.0, 1.0);
    }

    /// As 12 formas da Lente sobre um pedaço real da tela, no tamanho de 100%, ampliadas 3x.
    /// Uso: FLOWCURSOR_TESTE="x,y,pasta" cargo test --release formas_lente -- --ignored
    #[test]
    #[ignore]
    fn formas_lente() {
        let arg = std::env::var("FLOWCURSOR_TESTE").expect("FLOWCURSOR_TESTE=x,y,pasta");
        let partes: Vec<&str> = arg.splitn(3, ',').collect();
        let (x0, y0): (i32, i32) = (partes[0].parse().unwrap(), partes[1].parse().unwrap());
        let f = crate::captura::de_tela(x0, y0);
        let cfg = Config { tema: "lente".into(), ..Default::default() };
        let tema = temas::carregar("lente", &cfg);
        const C: usize = 96;
        const Z: usize = 3;
        let (w, h) = (C * 4, C * 3);
        let mut tela = vec![[0.0f32; 3]; w * h];
        for j in 0..h {
            for i in 0..w {
                tela[j * w + i] = f.em(x0 + i as i32, y0 + j as i32).map_or([0.0; 3], |c| c.0);
            }
        }
        let tipos = [Tipo::Seta, Tipo::Mao, Tipo::Texto, Tipo::Espera, Tipo::SetaEspera, Tipo::Cruz, Tipo::Proibido, Tipo::Mover, Tipo::RedimH, Tipo::RedimV, Tipo::RedimNwse, Tipo::RedimNesw];
        for (n, &tipo) in tipos.iter().enumerate() {
            let mut sp = Sprite::novo();
            let escala = fator_lente(true, tipo, 1.0);
            sp.renderizar(&[Camada { tipo, alfa: 1.0, escala }], &tema, 50.0, 0.35);
            let (cx, cy) = ((n % 4 * C) as i32 + 40, (n / 4 * C) as i32 + 40);
            let (ox, oy) = (x0 + cx - sp.ox, y0 + cy - sp.oy);
            let mut dest = sp.px.clone();
            vidro_por_baixo(&sp, 0, 0, (ox, oy, sp.w, sp.h), &f, true, 1.12, &mut dest);
            for j in 0..sp.h {
                for i in 0..sp.w {
                    let (x, y) = (cx - sp.ox + i as i32, cy - sp.oy + j as i32);
                    if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                        let d = dest[j * sp.w + i];
                        let t = &mut tela[y as usize * w + x as usize];
                        for k in 0..3 {
                            t[k] = d[k] + t[k] * (1.0 - d[3].min(1.0));
                        }
                    }
                }
            }
        }
        let mut out = vec![255u8; w * Z * h * Z * 4];
        for j in 0..h * Z {
            for i in 0..w * Z {
                let c = tela[(j / Z) * w + i / Z];
                let q = (j * w * Z + i) * 4;
                for k in 0..3 {
                    out[q + k] = (c[k].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
            }
        }
        crate::png::salvar(&std::path::Path::new(partes[2]).join("formas_lente.png"), w * Z, h * Z, &out).unwrap();
    }

    /// Pílula com a marca do mouse em fundo claro e escuro, ampliada 3x.
    /// Uso: FLOWCURSOR_TESTE="pasta" cargo test --release pilula_com_mouse -- --ignored
    #[test]
    #[ignore]
    fn pilula_com_mouse() {
        let pasta = std::env::var("FLOWCURSOR_TESTE").expect("FLOWCURSOR_TESTE=pasta");
        let cfg = Config { tema: "lente".into(), ..Default::default() };
        let tema = temas::carregar("lente", &cfg);
        let (s, pw, ph) = (1.5f32, 340.0f32, 60.0f32);
        let mut sp = Sprite::novo();
        sp.renderizar_pilula(pw, ph, s, &tema, 50.0);
        const Z: usize = 3;
        let (w, h) = (sp.w * Z, sp.h * 2 * Z);
        let mut out = vec![255u8; w * h * 4];
        for (n, fundo) in [[0.93f32, 0.92, 0.90], [0.16, 0.16, 0.19]].iter().enumerate() {
            let mut c = sp.px.clone();
            for (i, d) in c.iter_mut().enumerate() {
                let cob = sp.vidro[i][0];
                *d = sobre(*d, [fundo[0] * cob, fundo[1] * cob, fundo[2] * cob, cob.min(1.0)]);
            }
            marcar_mouse(&mut c, sp.w, sp.h, sp.ox as f32 + pw - 34.0, sp.oy as f32 + ph / 2.0, s);
            for j in 0..sp.h * Z {
                for i in 0..w {
                    let d = c[(j / Z) * sp.w + i / Z];
                    let q = ((j + n * sp.h * Z) * w + i) * 4;
                    for k in 0..3 {
                        out[q + k] = ((d[k] + fundo[k] * (1.0 - d[3].min(1.0))).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    }
                }
            }
        }
        crate::png::salvar(&std::path::Path::new(&pasta).join("pilula_mouse.png"), w, h, &out).unwrap();
    }

    /// Antes e depois da Lente líquida sobre um pedaço real da tela, ampliado 4x para ver os
    /// pixels. Uso: FLOWCURSOR_TESTE="x,y,pasta" cargo test --release lente_antes_depois -- --ignored
    #[test]
    #[ignore]
    fn lente_antes_depois() {
        let arg = std::env::var("FLOWCURSOR_TESTE").expect("FLOWCURSOR_TESTE=x,y,pasta");
        let partes: Vec<&str> = arg.splitn(3, ',').collect();
        let (x0, y0): (i32, i32) = (partes[0].parse().unwrap(), partes[1].parse().unwrap());
        let f = crate::captura::de_tela(x0, y0);
        let cfg = Config { tema: "lente".into(), ..Default::default() };
        let tema = temas::carregar("lente", &cfg);
        let mut lado_a_lado = Vec::new();
        for novo in [false, true] {
            // antes: slider em 185% (o que o Átila usava); depois: 100% (seta 1,85x)
            let dpi = crate::sistema::escala_dpi(crate::ffi::POINT { x: x0 + 150, y: y0 + 150 });
            let escala = dpi * if novo { fator_lente(true, Tipo::Seta, 1.0) } else { 1.85 };
            let mut sp = Sprite::novo();
            sp.renderizar(&[Camada { tipo: Tipo::Seta, alfa: 1.0, escala }], &tema, 50.0, 0.0);
            let (hx, hy) = (x0 + 150, y0 + 150);
            let (ox, oy) = (hx - sp.ox, hy - sp.oy);
            let mut dest = sp.px.clone();
            if novo {
                vidro_por_baixo(&sp, 0, 0, (ox, oy, sp.w, sp.h), &f, true, 1.12, &mut dest);
            } else {
                // a lente antiga: amplia 1,4x pegando o pixel mais próximo
                let centro = v2(ox as f32 + sp.centro.x, oy as f32 + sp.centro.y);
                for j in 0..sp.h {
                    for i in 0..sp.w {
                        let [cob, borda] = sp.vidro[j * sp.w + i];
                        if cob <= 0.0 {
                            continue;
                        }
                        let p = v2((ox + i as i32) as f32 + 0.5, (oy + j as i32) as f32 + 0.5);
                        let o = centro + (p - centro) / (1.4 * (1.0 + 0.12 * borda));
                        let Some((c, _)) = f.em(o.x.floor() as i32, o.y.floor() as i32) else { continue };
                        let d = &mut dest[j * sp.w + i];
                        *d = sobre(*d, [c[0] * cob, c[1] * cob, c[2] * cob, cob.min(1.0)]);
                    }
                }
            }
            // recorte de 120 px em volta da ponta, sobre a tela, ampliado 4x
            const R: usize = 120;
            const Z: usize = 4;
            let mut img = vec![0u8; R * Z * R * Z * 4];
            for j in 0..R {
                for i in 0..R {
                    let (sx, sy) = (hx - 20 + i as i32, hy - 20 + j as i32);
                    let fundo = f.em(sx, sy).map_or([0.0; 3], |c| c.0);
                    let (si, sj) = (sx - ox, sy - oy);
                    let d = if si >= 0 && sj >= 0 && (si as usize) < sp.w && (sj as usize) < sp.h { dest[sj as usize * sp.w + si as usize] } else { [0.0; 4] };
                    let c = [0, 1, 2].map(|k| ((d[k] + fundo[k] * (1.0 - d[3].min(1.0))).clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
                    for dy in 0..Z {
                        for dx in 0..Z {
                            let q = ((j * Z + dy) * R * Z + i * Z + dx) * 4;
                            img[q..q + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
                        }
                    }
                }
            }
            lado_a_lado.push(img);
        }
        // as duas lado a lado, com 8 px de separação
        let (w1, h1) = (480, 480);
        let w = w1 * 2 + 8;
        let mut out = vec![255u8; w * h1 * 4];
        for (k, img) in lado_a_lado.iter().enumerate() {
            for j in 0..h1 {
                let a = (j * w + k * (w1 + 8)) * 4;
                out[a..a + w1 * 4].copy_from_slice(&img[j * w1 * 4..(j + 1) * w1 * 4]);
            }
        }
        let pasta = std::path::Path::new(partes[2]);
        crate::png::salvar(&pasta.join("lente_antes_depois.png"), w, h1, &out).unwrap();
    }
}
