// Laço de desenho: uma volta por atualização do monitor. Lê a posição e a forma
// do ponteiro real, avança o motor e mostra o quadro na janela transparente.
use crate::config::Config;
use crate::ffi::*;
use crate::formas::v2;
use crate::motor::Motor;
use crate::overlay::Overlay;
use crate::reg;
use crate::sistema::{self, MapaCursores, Vblank, SITUACAO, SITUACAO_NORMAL};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub static SAIR: AtomicBool = AtomicBool::new(false);
pub static PAUSADO: AtomicBool = AtomicBool::new(false);
pub static BLOQUEADO: AtomicBool = AtomicBool::new(false);
pub static REAPLICAR: AtomicBool = AtomicBool::new(false);
pub static REABRIR_VBLANK: AtomicBool = AtomicBool::new(false);
pub static DESATIVAR_TELA_CHEIA: AtomicBool = AtomicBool::new(true);
static CONFIG: Mutex<Option<Config>> = Mutex::new(None);
/// Quanto tempo o ponteiro pode sumir sem o desenho apagar (evita piscar ao digitar).
const TOLERANCIA_OCULTO: Duration = Duration::from_millis(150);
/// Com cursor próprio de programa os dois apareceriam juntos, então a espera é menor.
const TOLERANCIA_CURSOR_PROPRIO: Duration = Duration::from_millis(40);
static CONFIG_VERSAO: AtomicU32 = AtomicU32::new(0);

pub fn publicar_config(c: Config) {
    DESATIVAR_TELA_CHEIA.store(c.desativar_tela_cheia, Relaxed);
    crate::alternador::HABILITADO.store(c.alternador, Relaxed);
    *CONFIG.lock().unwrap_or_else(|e| e.into_inner()) = Some(c);
    CONFIG_VERSAO.fetch_add(1, Relaxed);
}

pub fn config_atual() -> Config {
    CONFIG.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default()
}

/// Números do desempenho, registrados a cada 30 s de uso.
#[derive(Default)]
struct Estatisticas {
    inicio: Option<Instant>,
    voltas: u32,
    desenhados: u32,
    reerguidos: u32,
    soma_trabalho: Duration,
    max_trabalho: Duration,
    max_vel: f32,
    max_atraso: f32,
}

impl Estatisticas {
    fn registrar(&mut self, trabalho: Duration, motor: &Motor) {
        let inicio = *self.inicio.get_or_insert_with(Instant::now);
        self.voltas += 1;
        self.soma_trabalho += trabalho;
        self.max_trabalho = self.max_trabalho.max(trabalho);
        self.max_vel = self.max_vel.max(motor.velocidade());
        self.max_atraso = self.max_atraso.max(motor.atraso());
        let decorrido = inicio.elapsed();
        if decorrido >= Duration::from_secs(30) {
            let s = decorrido.as_secs_f64();
            reg!(
                "desempenho: {:.0} voltas/s, {:.0} quadros desenhados/s, trabalho médio {:.2} ms (máx {:.2} ms), \
                 velocidade máx {:.0} px/s, maior distância do ponteiro real {:.1} px, voltas ao topo {}",
                self.voltas as f64 / s,
                self.desenhados as f64 / s,
                self.soma_trabalho.as_secs_f64() * 1000.0 / self.voltas.max(1) as f64,
                self.max_trabalho.as_secs_f64() * 1000.0,
                self.max_vel,
                self.max_atraso,
                self.reerguidos
            );
            *self = Estatisticas::default();
        }
    }
}

pub fn executar() {
    unsafe {
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
        timeBeginPeriod(1);
    }
    let Some(mut janela) = Overlay::criar() else {
        reg!("não consegui criar a janela do ponteiro; encerrando");
        SAIR.store(true, Relaxed);
        return;
    };
    let mapa = MapaCursores::novo();
    let mut vblank = Vblank::abrir();
    reg!("sincronia com o monitor: {}", vblank.metodo);
    let mut motor = Motor::novo();
    let mut cfg = config_atual();
    let mut versao = CONFIG_VERSAO.load(Relaxed);
    let mut escondidos = false;
    let inicio = Instant::now();
    let mut ultima_conferencia = Instant::now();
    let mut desconhecidos: HashSet<isize> = HashSet::new();
    let mut est = Estatisticas::default();
    let mut dpi: (HANDLE, f32) = (0, 1.0);
    let mut sumico: Option<(Instant, u32, isize)> = None;
    let mut sumicos_registrados = 0u32;
    let mut vistas_acima: HashSet<isize> = HashSet::new();

    while !SAIR.load(Relaxed) {
        vblank.esperar();
        let comeco = Instant::now();
        janela.bombear();
        if REABRIR_VBLANK.swap(false, Relaxed) {
            vblank = Vblank::abrir();
            dpi = (0, 1.0);
        }
        let v = CONFIG_VERSAO.load(Relaxed);
        if v != versao {
            // sem resetar o motor: na tela de ajustes a mudança aparece enquanto o ponteiro anda
            versao = v;
            cfg = config_atual();
        }

        // Pausado, tela bloqueada, menu Iniciar, tela cheia...: ponteiro normal do Windows.
        let passar = PAUSADO.load(Relaxed) || BLOQUEADO.load(Relaxed) || SITUACAO.load(Relaxed) != SITUACAO_NORMAL;
        if passar {
            if escondidos {
                sistema::restaurar_cursores();
                escondidos = false;
            }
            janela.esconder();
            motor.resetar();
            continue;
        }
        if !escondidos || REAPLICAR.swap(false, Relaxed) {
            if !sistema::esconder_cursores() {
                reg!("aviso: nem todos os cursores do sistema foram trocados");
            }
            escondidos = true;
            ultima_conferencia = Instant::now();
        } else if ultima_conferencia.elapsed() > Duration::from_secs(2) {
            // Mudar o esquema de ponteiros ou o tamanho nas Configurações devolve os cursores.
            ultima_conferencia = Instant::now();
            if !sistema::seta_em_branco() {
                reg!("os cursores do sistema voltaram (outro programa ou Configurações); escondendo de novo");
                sistema::esconder_cursores();
            }
        }

        let mut ci = CURSORINFO { cbSize: std::mem::size_of::<CURSORINFO>() as u32, flags: 0, hCursor: 0, ptScreenPos: POINT::default() };
        unsafe { GetCursorInfo(&mut ci) };
        let visivel = ci.flags & CURSOR_SHOWING != 0;
        let Some(tipo) = (if visivel { mapa.tipo(ci.hCursor) } else { None }) else {
            // Programa escondeu o ponteiro, ou usa um desenho próprio (que continua aparecendo).
            // Com "ocultar ponteiro ao digitar", o Windows e alguns programas escondem e mostram
            // de novo em poucos milissegundos a cada tecla; esse sumiço curto não apaga o desenho.
            let (desde, _, _) = *sumico.get_or_insert((Instant::now(), ci.flags, ci.hCursor));
            let tolerancia = if visivel { TOLERANCIA_CURSOR_PROPRIO } else { TOLERANCIA_OCULTO };
            if desde.elapsed() >= tolerancia {
                janela.esconder();
                motor.resetar();
            }
            if visivel && ci.hCursor != 0 && desconhecidos.len() < 100 && desconhecidos.insert(ci.hCursor) {
                let fg = unsafe { GetForegroundWindow() };
                reg!("cursor próprio de programa, desenho real mantido: {:#x} em {}", ci.hCursor, sistema::processo(fg));
            }
            continue;
        };
        if let Some((desde, flags, cursor)) = sumico.take() {
            let ms = desde.elapsed().as_millis();
            if ms < 2000 && sumicos_registrados < 60 {
                sumicos_registrados += 1;
                let fg = unsafe { GetForegroundWindow() };
                let tolerancia = if flags & CURSOR_SHOWING != 0 { TOLERANCIA_CURSOR_PROPRIO } else { TOLERANCIA_OCULTO };
                reg!(
                    "ponteiro sumiu por {ms} ms (visível={}, cursor={cursor:#x}) em {}{}",
                    flags & CURSOR_SHOWING != 0,
                    sistema::processo(fg),
                    if desde.elapsed() < tolerancia { ": desenho mantido" } else { "" }
                );
            }
        }

        let mut pt = POINT::default();
        unsafe { GetCursorPos(&mut pt) };
        let mon = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) };
        if mon != dpi.0 {
            dpi = (mon, sistema::escala_dpi(pt));
        }
        let pressionado = unsafe { GetAsyncKeyState(VK_LBUTTON) < 0 || GetAsyncKeyState(VK_RBUTTON) < 0 };
        let t = inicio.elapsed().as_secs_f64();
        if let Some(q) = motor.quadro(t, v2(pt.x as f32, pt.y as f32), tipo, pressionado, dpi.1, &cfg) {
            janela.apresentar(&q);
            est.desenhados += 1;
        }
        if let Some((acima, barreira)) = janela.manter_no_topo() {
            est.reerguidos += 1;
            if vistas_acima.len() < 50 && vistas_acima.insert(acima) {
                reg!("janela passou na frente do ponteiro: {} ({})", sistema::processo(acima), sistema::classe(acima));
            }
            if let Some(b) = barreira {
                reg!("camada do Windows acima do ponteiro (não dá para passar): {} ({})", sistema::processo(b), sistema::classe(b));
            }
        }
        est.registrar(comeco.elapsed(), &motor);
    }

    drop(janela);
    if escondidos {
        sistema::restaurar_cursores();
    }
    unsafe { timeEndPeriod(1) };
    reg!("laço de desenho encerrado, ponteiro normal devolvido");
}
