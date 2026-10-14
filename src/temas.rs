// Temas de ponteiro, de três origens:
//   - vetoriais do FlowCursor: desenhados por campos de distância, nítidos em qualquer tamanho;
//   - Windows: os cursores do esquema do Windows (.cur e .ani), o atual ou outro instalado;
//   - pastas do usuário em <pasta do flowcursor.ini>\cursores, com .cur, .ani ou .png.
// O que faltar num tema de imagens é desenhado pela forma vetorial de cor parecida.
use crate::config::{self, Config};
use crate::ffi::*;
use crate::formas::{v2, Camada, Cor, Forma, Paleta, Sprite, Tipo, Vetorial, V2, CLARO, MACOS};
use crate::reg;
use std::collections::HashMap;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

// ---------- Imagens ----------

/// Uma imagem de cursor em cores pré-multiplicadas.
pub struct Bitmap {
    w: usize,
    h: usize,
    /// Ponto ativo, em pixels da imagem.
    hx: f32,
    hy: f32,
    /// Pixels da imagem por pixel do ponteiro a 100% (imagem de 128 px num cursor de 32 px: 4).
    unidade: f32,
    px: Vec<Cor>,
}

impl Bitmap {
    /// Cantos da imagem em coordenadas do ponteiro a 100%, com o ponto ativo na origem.
    pub fn extensao(&self) -> (V2, V2) {
        let k = 1.0 / self.unidade;
        (v2(-self.hx * k, -self.hy * k), v2((self.w as f32 - self.hx) * k, (self.h as f32 - self.hy) * k))
    }

    fn texel(&self, x: i32, y: i32) -> Cor {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return [0.0; 4];
        }
        self.px[y as usize * self.w + x as usize]
    }

    fn bilinear(&self, u: f32, v: f32) -> Cor {
        let (x, y) = (u - 0.5, v - 0.5);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (x0, y0) = (x0 as i32, y0 as i32);
        let mut c = [0.0; 4];
        for (dx, dy, p) in [(0, 0, (1.0 - fx) * (1.0 - fy)), (1, 0, fx * (1.0 - fy)), (0, 1, (1.0 - fx) * fy), (1, 1, fx * fy)] {
            if p > 0.0 {
                let t = self.texel(x0 + dx, y0 + dy);
                for k in 0..4 {
                    c[k] += t[k] * p;
                }
            }
        }
        c
    }

    /// Cor no ponto `p` (coordenadas do ponteiro a 100%) para uma tela com `s` pixels por
    /// unidade. Ao reduzir, tira a média de vários pontos, para a borda não serrilhar.
    pub fn amostrar(&self, p: V2, s: f32) -> Cor {
        let (u, v) = (self.hx + p.x * self.unidade, self.hy + p.y * self.unidade);
        let f = self.unidade / s;
        let n = f.ceil().clamp(1.0, 8.0) as usize;
        if n <= 1 {
            return self.bilinear(u, v);
        }
        let mut c = [0.0; 4];
        for b in 0..n {
            for a in 0..n {
                let du = ((a as f32 + 0.5) / n as f32 - 0.5) * f;
                let dv = ((b as f32 + 0.5) / n as f32 - 0.5) * f;
                let t = self.bilinear(u + du, v + dv);
                for k in 0..4 {
                    c[k] += t[k];
                }
            }
        }
        let k = 1.0 / (n * n) as f32;
        c.map(|x| x * k)
    }

    /// Opacidade desfocada em volta de `p` (para a sombra), com raio em unidades do ponteiro.
    pub fn alfa_borrado(&self, p: V2, raio: f32) -> f32 {
        let (u, v) = (self.hx + p.x * self.unidade, self.hy + p.y * self.unidade);
        let r = raio * self.unidade * 0.5;
        let (mut soma, mut pesos) = (0.0, 0.0);
        for b in -2..=2 {
            for a in -2..=2 {
                let peso = (-0.5 * (a * a + b * b) as f32).exp();
                soma += self.bilinear(u + a as f32 * r, v + b as f32 * r)[3] * peso;
                pesos += peso;
            }
        }
        soma / pesos
    }

    /// Luminância média do que é opaco (para escolher a cor das formas que faltam).
    fn clara(&self) -> bool {
        let (mut soma, mut n) = (0.0, 0.0);
        for c in &self.px {
            if c[3] > 0.6 {
                soma += (0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]) / c[3];
                n += 1.0;
            }
        }
        n == 0.0 || soma / n > 0.5
    }
}

/// Os quadros de um cursor (um só, se não for animado) e por quanto tempo cada um fica.
struct Animacao {
    quadros: Vec<Bitmap>,
    passos: Vec<(usize, f32)>,
    total: f32,
}

impl Animacao {
    fn passo(&self, tempo: f32) -> usize {
        if self.passos.len() <= 1 {
            return 0;
        }
        let mut t = tempo.rem_euclid(self.total);
        for (i, (_, d)) in self.passos.iter().enumerate() {
            if t < *d {
                return i;
            }
            t -= d;
        }
        self.passos.len() - 1
    }
}

// ---------- Tema ----------

static SERIE: AtomicU64 = AtomicU64::new(1);

pub struct Tema {
    /// Muda a cada tema carregado: o motor redesenha o sprite quando muda.
    pub serie: u64,
    /// Formas vetoriais (o tema inteiro, ou o que faltar num tema de imagens).
    pub vetor: Vetorial,
    imagens: HashMap<Tipo, Animacao>,
}

impl Tema {
    fn novo(vetor: Vetorial, imagens: HashMap<Tipo, Animacao>) -> Tema {
        Tema { serie: SERIE.fetch_add(1, Relaxed), vetor, imagens }
    }

    pub fn imagem(&self, tipo: Tipo, tempo: f32) -> Option<&Bitmap> {
        let a = self.imagens.get(&tipo)?;
        let (q, _) = a.passos[a.passo(tempo)];
        a.quadros.get(q)
    }

    /// Número que muda quando o desenho animado de `tipo` muda.
    pub fn fase(&self, tipo: Tipo, tempo: f32) -> u32 {
        match self.imagens.get(&tipo) {
            Some(a) => a.passo(tempo) as u32,
            None if tipo.animado() => ((tempo * 1.2).fract() * 720.0) as u32,
            None => 0,
        }
    }

    pub fn paleta(&self) -> Paleta {
        self.vetor.pal
    }
}

// ---------- Catálogo ----------

pub struct Info {
    pub id: String,
    pub nome: String,
    pub grupo: &'static str,
    pub detalhe: String,
}

/// Temas vetoriais, na ordem da tela de ajustes.
const VETORIAIS: [(&str, &str, &str); 7] = [
    ("macos", "macOS", "Preto com borda branca"),
    ("claro", "Claro", "Branco com borda preta"),
    ("bibata", "Bibata", "Arredondado, do Linux"),
    ("bibata_gelo", "Bibata Gelo", "Arredondado e branco"),
    ("ipados", "iPadOS", "Círculo translúcido do iPad"),
    ("aurora", "Aurora", "Degradê nas cores do FlowCursor"),
    ("cores", "Suas cores", "Você escolhe o miolo e a borda"),
];

pub fn cor_hex(s: &str) -> Option<Cor> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(s, 16).ok()?;
    let c = |k: u32| ((n >> k) & 255) as f32 / 255.0;
    Some([c(16), c(8), c(0), 1.0])
}

fn vetorial(id: &str, cfg: &Config) -> Option<Vetorial> {
    let pal = |preenchimento: Cor, contorno: Cor, largura: f32| Paleta { preenchimento, contorno, largura };
    let (branco, preto) = ([1.0, 1.0, 1.0, 1.0], [0.06, 0.06, 0.07, 1.0]);
    Some(match id {
        "macos" => MACOS,
        "claro" => CLARO,
        "bibata" => Vetorial { forma: Forma::Arredondada, pal: pal(preto, branco, 1.7), degrade: None },
        "bibata_gelo" => Vetorial { forma: Forma::Arredondada, pal: pal(branco, preto, 1.45), degrade: None },
        "ipados" => Vetorial { forma: Forma::Circulo, pal: pal([0.3, 0.3, 0.34, 0.6], [1.0, 1.0, 1.0, 0.5], 0.8), degrade: None },
        "aurora" => Vetorial {
            forma: Forma::Arredondada,
            pal: pal([0.369, 0.361, 0.902, 1.0], branco, 1.6),
            degrade: Some([[0.353, 0.784, 0.98], [0.369, 0.361, 0.902], [0.749, 0.353, 0.949]]),
        },
        "cores" => Vetorial {
            forma: Forma::Classica,
            pal: pal(cor_hex(&cfg.cor_preenchimento).unwrap_or(preto), cor_hex(&cfg.cor_contorno).unwrap_or(branco), 1.5),
            degrade: None,
        },
        _ => return None,
    })
}

pub fn listar() -> Vec<Info> {
    let mut v: Vec<Info> = VETORIAIS
        .iter()
        .map(|(id, nome, d)| Info { id: id.to_string(), nome: nome.to_string(), grupo: "FlowCursor", detalhe: d.to_string() })
        .collect();
    v.push(Info { id: "windows".into(), nome: "Windows".into(), grupo: "Windows", detalhe: "Os originais, do esquema atual".into() });
    for (nome, _) in esquemas_instalados() {
        v.push(Info { id: format!("esquema:{nome}"), nome, grupo: "Windows", detalhe: "Esquema instalado no Windows".into() });
    }
    for (nome, _) in pastas() {
        v.push(Info { id: format!("pasta:{nome}"), nome, grupo: "Seus", detalhe: "Da pasta cursores".into() });
    }
    v
}

/// Monta o tema pelo identificador gravado no .ini; se não existir mais, volta o macOS.
pub fn carregar(id: &str, cfg: &Config) -> Tema {
    if let Some(v) = vetorial(id, cfg) {
        return Tema::novo(v, HashMap::new());
    }
    let arquivos = if id == "windows" {
        Some(esquema_atual())
    } else if let Some(nome) = id.strip_prefix("esquema:") {
        esquemas_instalados().into_iter().find(|(n, _)| n == nome).map(|(_, lista)| de_lista(&lista))
    } else if let Some(nome) = id.strip_prefix("pasta:") {
        pastas().into_iter().find(|(n, _)| n == nome).map(|(_, dir)| arquivos_da_pasta(&dir))
    } else {
        None
    };
    let Some((arquivos, opcoes)) = arquivos.map(|a| match a {
        Origem::Registro(v) => (v, Opcoes::default()),
        Origem::Pasta(v, o) => (v, o),
    }) else {
        reg!("tema '{id}' não encontrado; usando o macOS");
        return Tema::novo(MACOS, HashMap::new());
    };
    let mut imagens = HashMap::new();
    for (tipo, caminho) in arquivos {
        match ler_animacao(&caminho, tipo, &opcoes) {
            Some(a) => {
                imagens.insert(tipo, a);
            }
            None => reg!("tema '{id}': não consegui ler {}", caminho.display()),
        }
    }
    // o que faltar sai na forma vetorial com a cor da seta do tema
    let clara = imagens.get(&Tipo::Seta).and_then(|a| a.quadros.first()).is_none_or(Bitmap::clara);
    Tema::novo(if clara { CLARO } else { MACOS }, imagens)
}

struct Cache {
    id: String,
    cores: (String, String),
    revisao: u64,
    tema: Arc<Tema>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);
static REVISAO: AtomicU64 = AtomicU64::new(0);

/// Tema da configuração, carregado uma vez e guardado até mudar.
pub fn obter(cfg: &Config) -> Arc<Tema> {
    let revisao = REVISAO.load(Relaxed);
    let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(x) = c.as_ref() {
        if x.id == cfg.tema && x.revisao == revisao && (cfg.tema != "cores" || (x.cores.0 == cfg.cor_preenchimento && x.cores.1 == cfg.cor_contorno)) {
            return x.tema.clone();
        }
    }
    let tema = Arc::new(carregar(&cfg.tema, cfg));
    *c = Some(Cache { id: cfg.tema.clone(), cores: (cfg.cor_preenchimento.clone(), cfg.cor_contorno.clone()), revisao, tema: tema.clone() });
    tema
}

/// Lê de novo os arquivos do tema (o esquema do Windows ou a pasta podem ter mudado).
pub fn recarregar() {
    REVISAO.fetch_add(1, Relaxed);
}

// ---------- Origens dos arquivos ----------

#[derive(Default)]
struct Opcoes {
    /// Multiplica o tamanho das imagens (tema.ini).
    escala: f32,
    /// Ponto ativo por forma, em pixels da imagem (tema.ini); vale para .png e corrige .cur.
    pontos: HashMap<Tipo, (f32, f32)>,
}

impl Opcoes {
    fn escala(&self) -> f32 {
        if self.escala > 0.0 {
            self.escala
        } else {
            1.0
        }
    }
}

enum Origem {
    Registro(Vec<(Tipo, PathBuf)>),
    Pasta(Vec<(Tipo, PathBuf)>, Opcoes),
}

/// Nome de cada forma no registro do Windows (HKCU\Control Panel\Cursors).
const NOMES_REGISTRO: [(&str, Tipo); 12] = [
    ("Arrow", Tipo::Seta),
    ("IBeam", Tipo::Texto),
    ("Hand", Tipo::Mao),
    ("Wait", Tipo::Espera),
    ("AppStarting", Tipo::SetaEspera),
    ("Crosshair", Tipo::Cruz),
    ("No", Tipo::Proibido),
    ("SizeWE", Tipo::RedimH),
    ("SizeNS", Tipo::RedimV),
    ("SizeNWSE", Tipo::RedimNwse),
    ("SizeNESW", Tipo::RedimNesw),
    ("SizeAll", Tipo::Mover),
];

/// Ordem dos arquivos num esquema do Windows (registro e install.inf dos pacotes).
const ORDEM_ESQUEMA: [Option<Tipo>; 15] = [
    Some(Tipo::Seta),
    None, // ajuda
    Some(Tipo::SetaEspera),
    Some(Tipo::Espera),
    Some(Tipo::Cruz),
    Some(Tipo::Texto),
    None, // caneta
    Some(Tipo::Proibido),
    Some(Tipo::RedimV),
    Some(Tipo::RedimH),
    Some(Tipo::RedimNwse),
    Some(Tipo::RedimNesw),
    Some(Tipo::Mover),
    None, // seta para cima
    Some(Tipo::Mao),
];

fn expandir(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let mut buf = vec![0u16; 1024];
    let n = unsafe { ExpandEnvironmentStringsW(w(s).as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
    if n == 0 || n as usize > buf.len() {
        return s.to_string();
    }
    ler_w(&buf)
}

const REG_EXPAND_SZ: u32 = 2;

/// Todos os valores de texto de uma chave do registro.
fn valores(raiz: HANDLE, sub: &str) -> Vec<(String, String)> {
    let mut v = Vec::new();
    unsafe {
        let mut chave: HANDLE = 0;
        if RegOpenKeyExW(raiz, w(sub).as_ptr(), 0, KEY_QUERY_VALUE, &mut chave) != 0 {
            return v;
        }
        for i in 0..512 {
            let mut nome = [0u16; 512];
            let mut tam_nome = nome.len() as u32;
            let mut dados = vec![0u16; 8192];
            let mut tam = (dados.len() * 2) as u32;
            let mut tipo = 0u32;
            let r = RegEnumValueW(chave, i, nome.as_mut_ptr(), &mut tam_nome, null_mut(), &mut tipo, dados.as_mut_ptr().cast(), &mut tam);
            if r == 259 {
                break; // ERROR_NO_MORE_ITEMS
            }
            if r != 0 || (tipo != REG_SZ && tipo != REG_EXPAND_SZ) {
                continue;
            }
            let texto = ler_w(&dados[..(tam as usize / 2).min(dados.len())]);
            v.push((String::from_utf16_lossy(&nome[..tam_nome as usize]), texto));
        }
        RegCloseKey(chave);
    }
    v
}

fn esquema_atual() -> Origem {
    let vals = valores(HKEY_CURRENT_USER, r"Control Panel\Cursors");
    let mut v = Vec::new();
    for (nome, tipo) in NOMES_REGISTRO {
        if let Some((_, c)) = vals.iter().find(|(n, _)| n.eq_ignore_ascii_case(nome)) {
            if !c.trim().is_empty() {
                v.push((tipo, PathBuf::from(expandir(c.trim()))));
            }
        }
    }
    Origem::Registro(v)
}

/// Esquemas de cursores salvos pelo usuário no Windows (os pacotes instalados ficam aqui).
/// Os do sistema (Aero, Black, Inverted) não entram: o atual já cobre o Aero.
fn esquemas_instalados() -> Vec<(String, String)> {
    let mut v = valores(HKEY_CURRENT_USER, r"Control Panel\Cursors\Schemes");
    v.retain(|(_, lista)| lista.contains(','));
    v.sort_by_key(|(n, _)| n.to_lowercase());
    v
}

fn de_lista(lista: &str) -> Origem {
    let mut v = Vec::new();
    for (i, item) in lista.split(',').enumerate() {
        if let Some(Some(tipo)) = ORDEM_ESQUEMA.get(i) {
            let c = item.trim().trim_matches('"');
            if !c.is_empty() {
                v.push((*tipo, PathBuf::from(expandir(c))));
            }
        }
    }
    Origem::Registro(v)
}

// ---------- Pastas do usuário ----------

pub fn pasta_temas() -> PathBuf {
    let base = config::CAMINHO
        .get()
        .and_then(|c| c.parent().map(Path::to_path_buf))
        .or_else(|| std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)))
        .unwrap_or_default();
    base.join("cursores")
}

pub const LEIA_ME: &str = "\
Temas do FlowCursor
===================

Cada pasta aqui dentro vira um tema na tela de ajustes (Aparencia > Tema).

Jeito mais facil: baixe um pacote de cursores para Windows (.zip), descompacte
e coloque a pasta aqui. Se o pacote tiver um install.inf, o FlowCursor le dele
qual arquivo e qual ponteiro.

Sem install.inf, de aos arquivos (.cur, .ani ou .png) o nome do ponteiro:
  seta        normal, arrow
  mao         link, hand
  texto       text, ibeam
  espera      busy, wait
  seta_espera working, appstarting
  cruz        precision, crosshair
  proibido    unavailable, no
  redim_h     horizontal, sizewe
  redim_v     vertical, sizens
  redim_nwse  diagonal1, sizenwse
  redim_nesw  diagonal2, sizenesw
  mover       move, sizeall

O que faltar e desenhado pelo FlowCursor.

Opcional: um arquivo tema.ini na pasta do tema com
  escala = 1.0          # aumenta ou diminui as imagens do tema
  ponto_seta = 3,2      # ponto do clique em pixels da imagem (util para .png)
  ponto_mao = 10,1

Uma imagem .png inteira vale um ponteiro de 32 pixels a 100%.
Depois de mexer nos arquivos, escolha o tema de novo na tela de ajustes.
";

/// Pastas de temas (só as que têm algum cursor).
fn pastas() -> Vec<(String, PathBuf)> {
    let Ok(ler) = std::fs::read_dir(pasta_temas()) else { return Vec::new() };
    let mut v: Vec<(String, PathBuf)> = ler
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter(|e| !arquivos(&e.path()).is_empty())
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .collect();
    v.sort_by_key(|(n, _)| n.to_lowercase());
    v
}

/// Arquivos de cursor da pasta e das subpastas (pacotes costumam vir com uma pasta dentro).
fn arquivos(dir: &Path) -> Vec<PathBuf> {
    let mut v = Vec::new();
    let mut fila = vec![(dir.to_path_buf(), 0)];
    while let Some((d, nivel)) = fila.pop() {
        let Ok(ler) = std::fs::read_dir(&d) else { continue };
        let mut aqui: Vec<PathBuf> = ler.flatten().map(|e| e.path()).collect();
        aqui.sort();
        for p in aqui {
            if p.is_dir() {
                if nivel < 2 {
                    fila.push((p, nivel + 1));
                }
            } else if extensao(&p).is_some_and(|e| matches!(e.as_str(), "cur" | "ani" | "png" | "ico" | "inf")) {
                v.push(p);
            }
        }
    }
    v
}

fn extensao(p: &Path) -> Option<String> {
    p.extension().map(|e| e.to_string_lossy().to_lowercase())
}

/// Nome sem números do começo, sinais e maiúsculas: "01 - Normal Select" vira "normalselect".
fn normalizar(s: &str) -> String {
    let s: String = s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect();
    let sem_numero = s.trim_start_matches(|c: char| c.is_ascii_digit());
    if sem_numero.is_empty() { s } else { sem_numero.to_string() }
}

const APELIDOS: [(Tipo, &[&str]); 12] = [
    (Tipo::Seta, &["seta", "arrow", "normal", "normalselect", "pointer", "default", "leftptr", "cursor", "select"]),
    (Tipo::Texto, &["texto", "ibeam", "text", "textselect", "beam", "xterm"]),
    (Tipo::Mao, &["mao", "mão", "hand", "link", "linkselect", "pointinghand", "hand2", "handpointer"]),
    (Tipo::Espera, &["espera", "wait", "busy", "ocupado", "watch", "loading"]),
    (Tipo::SetaEspera, &["setaespera", "appstarting", "working", "workinginbackground", "background", "progress", "leftptrwatch", "halfbusy"]),
    (Tipo::Cruz, &["cruz", "crosshair", "cross", "precision", "precisionselect", "mira"]),
    (Tipo::Proibido, &["proibido", "no", "unavailable", "unavail", "notallowed", "forbidden", "nodrop"]),
    (Tipo::RedimH, &["redimh", "sizewe", "ew", "ewresize", "horizontal", "horizontalresize", "horz", "hdoublearrow", "size3"]),
    (Tipo::RedimV, &["redimv", "sizens", "ns", "nsresize", "vertical", "verticalresize", "vert", "vdoublearrow", "size4"]),
    (Tipo::RedimNwse, &["redimnwse", "sizenwse", "nwse", "nwseresize", "diagonal1", "diagonalresize1", "dgn1", "size2"]),
    (Tipo::RedimNesw, &["redimnesw", "sizenesw", "nesw", "neswresize", "diagonal2", "diagonalresize2", "dgn2", "size1"]),
    (Tipo::Mover, &["mover", "sizeall", "move", "allscroll", "fleur", "moveresize"]),
];

fn tipo_do_nome(nome: &str) -> Option<Tipo> {
    let n = normalizar(nome);
    APELIDOS.iter().find(|(_, a)| a.contains(&n.as_str())).map(|(t, _)| *t)
}

fn arquivos_da_pasta(dir: &Path) -> Origem {
    let todos = arquivos(dir);
    let mut opcoes = Opcoes::default();
    if let Ok(texto) = std::fs::read_to_string(dir.join("tema.ini")) {
        for linha in texto.lines() {
            let linha = linha.split('#').next().unwrap_or("");
            let Some((k, v)) = linha.split_once('=') else { continue };
            let (k, v) = (k.trim().to_lowercase(), v.trim());
            if k == "escala" {
                opcoes.escala = v.replace(',', ".").parse().unwrap_or(0.0);
            } else if let Some(tipo) = k.strip_prefix("ponto_").and_then(tipo_do_nome) {
                let n: Vec<f32> = v.split([',', ' ', ';']).filter_map(|x| x.trim().parse().ok()).collect();
                if n.len() >= 2 {
                    opcoes.pontos.insert(tipo, (n[0], n[1]));
                }
            }
        }
    }
    let cursores: Vec<&PathBuf> = todos.iter().filter(|p| extensao(p).as_deref() != Some("inf")).collect();
    let achar = |nome: &str| cursores.iter().find(|p| p.file_name().is_some_and(|f| f.to_string_lossy().eq_ignore_ascii_case(nome))).map(|p| (*p).clone());

    // 1) install.inf do pacote
    let mut v: Vec<(Tipo, PathBuf)> = Vec::new();
    for inf in todos.iter().filter(|p| extensao(p).as_deref() == Some("inf")) {
        let Ok(b) = std::fs::read(inf) else { continue };
        let texto = texto_de_inf(&b);
        for (i, nome) in lista_do_inf(&texto).iter().enumerate() {
            if let (Some(Some(tipo)), Some(c)) = (ORDEM_ESQUEMA.get(i), achar(nome)) {
                if !v.iter().any(|(t, _)| t == tipo) {
                    v.push((*tipo, c));
                }
            }
        }
    }
    // 2) nomes dos arquivos
    for p in &cursores {
        let Some(nome) = p.file_stem() else { continue };
        if let Some(tipo) = tipo_do_nome(&nome.to_string_lossy()) {
            if !v.iter().any(|(t, _)| *t == tipo) {
                v.push((tipo, (*p).clone()));
            }
        }
    }
    Origem::Pasta(v, opcoes)
}

/// install.inf costuma vir em ANSI ou UTF-16.
fn texto_de_inf(b: &[u8]) -> String {
    if b.len() >= 2 && b[0] == 0xFF && b[1] == 0xFE {
        let u: Vec<u16> = b[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        return String::from_utf16_lossy(&u);
    }
    match std::str::from_utf8(b) {
        Ok(s) => s.trim_start_matches('\u{feff}').to_string(),
        Err(_) => b.iter().map(|&c| c as char).collect(),
    }
}

/// Lista de arquivos do esquema no install.inf (linha "...Cursors\Schemes" do [Scheme.Reg]),
/// com as variáveis da seção [Strings] resolvidas. Só os nomes dos arquivos, na ordem do Windows.
fn lista_do_inf(texto: &str) -> Vec<String> {
    let mut strings: HashMap<String, String> = HashMap::new();
    let mut secao = String::new();
    for linha in texto.lines() {
        let l = linha.split(';').next().unwrap_or("").trim();
        if l.starts_with('[') {
            secao = l.trim_matches(['[', ']']).to_lowercase();
        } else if secao == "strings" {
            if let Some((k, v)) = l.split_once('=') {
                strings.insert(k.trim().to_lowercase(), v.trim().trim_matches('"').to_string());
            }
        }
    }
    let resolver = |s: &str| {
        let mut r = s.to_string();
        for _ in 0..4 {
            let partes: Vec<&str> = r.split('%').collect();
            if partes.len() < 3 {
                break;
            }
            let mut novo = String::new();
            for (i, p) in partes.iter().enumerate() {
                if i % 2 == 1 {
                    novo.push_str(strings.get(&p.to_lowercase()).map_or("", String::as_str));
                } else {
                    novo.push_str(p);
                }
            }
            r = novo;
        }
        r
    };
    for linha in texto.lines() {
        if !linha.to_lowercase().contains(r"cursors\schemes") {
            continue;
        }
        // o valor é o trecho entre aspas com mais vírgulas
        let Some(valor) = linha.split('"').skip(1).step_by(2).max_by_key(|t| t.matches(',').count()) else { continue };
        if !valor.contains(',') {
            continue;
        }
        return valor
            .split(',')
            .map(|item| {
                let r = resolver(item.trim());
                r.rsplit(['\\', '/']).next().unwrap_or("").trim().to_string()
            })
            .collect();
    }
    Vec::new()
}

// ---------- Leitura de .cur, .ani e .png ----------

const ASSINATURA_PNG: &[u8] = b"\x89PNG\r\n\x1a\n";

fn le16(b: &[u8], i: usize) -> u32 {
    b.get(i..i + 2).map_or(0, |x| u16::from_le_bytes([x[0], x[1]]) as u32)
}
fn le32(b: &[u8], i: usize) -> u32 {
    b.get(i..i + 4).map_or(0, |x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
}
fn be32(b: &[u8], i: usize) -> u32 {
    b.get(i..i + 4).map_or(0, |x| u32::from_be_bytes([x[0], x[1], x[2], x[3]]))
}

fn ler_animacao(caminho: &Path, tipo: Tipo, op: &Opcoes) -> Option<Animacao> {
    let b = std::fs::read(caminho).ok()?;
    if b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"ACON" {
        return ler_ani(&b, tipo, op);
    }
    let bmp = ler_imagem(&b, tipo, op)?;
    Some(Animacao { quadros: vec![bmp], passos: vec![(0, 1.0)], total: 1.0 })
}

/// Ponto ativo de quem não diz o seu (.png e .ico): a ponta nas setas, o meio nas outras.
fn ponto_padrao(tipo: Tipo, w: f32, h: f32) -> (f32, f32) {
    match tipo {
        Tipo::Seta | Tipo::SetaEspera => (0.0, 0.0),
        Tipo::Mao => (w * 0.4, 0.0),
        _ => (w / 2.0, h / 2.0),
    }
}

fn ler_imagem(b: &[u8], tipo: Tipo, op: &Opcoes) -> Option<Bitmap> {
    let ponto = |padrao: (f32, f32)| op.pontos.get(&tipo).copied().unwrap_or(padrao);
    if b.starts_with(ASSINATURA_PNG) {
        let (w, h) = (be32(b, 16) as usize, be32(b, 20) as usize);
        let px = decodificar(b, w, h)?;
        let (hx, hy) = ponto(ponto_padrao(tipo, w as f32, h as f32));
        let unidade = w.max(h) as f32 / 32.0 / op.escala();
        return Some(Bitmap { w, h, hx, hy, unidade, px });
    }
    // .cur e .ico: diretório com uma entrada por tamanho; fica a maior
    if b.len() < 6 || b[0] != 0 || b[1] != 0 || !(b[2] == 1 || b[2] == 2) || b[3] != 0 {
        return None;
    }
    let cursor = b[2] == 2;
    let n = le16(b, 4) as usize;
    let mut melhor: Option<(usize, u32, usize)> = None; // (largura, bits, índice)
    for i in 0..n {
        let e = 6 + 16 * i;
        if e + 16 > b.len() {
            break;
        }
        let largura = if b[e] == 0 { 256 } else { b[e] as usize };
        let bits = if cursor { 32 } else { le16(b, e + 6) };
        if melhor.is_none_or(|(l, bb, _)| (largura, bits) > (l, bb)) {
            melhor = Some((largura, bits, i));
        }
    }
    let (_, _, i) = melhor?;
    let e = 6 + 16 * i;
    let (mut w, mut h) = (if b[e] == 0 { 256 } else { b[e] as usize }, if b[e + 1] == 0 { 256 } else { b[e + 1] as usize });
    let (tam, ini) = (le32(b, e + 8) as usize, le32(b, e + 12) as usize);
    let dados = b.get(ini..ini.checked_add(tam)?)?;
    if dados.starts_with(ASSINATURA_PNG) {
        (w, h) = (be32(dados, 16) as usize, be32(dados, 20) as usize);
    }
    let px = decodificar(dados, w, h)?;
    let (hx, hy) = if cursor { (le16(b, e + 4) as f32, le16(b, e + 6) as f32) } else { ponto_padrao(tipo, w as f32, h as f32) };
    let (hx, hy) = ponto((hx, hy));
    // cursores do Windows: a imagem de 32 px é o tamanho a 100%
    Some(Bitmap { w, h, hx, hy, unidade: w as f32 / 32.0 / op.escala(), px })
}

/// .ani: arquivo RIFF com o cabeçalho "anih", a velocidade ("rate"), a ordem ("seq ")
/// e os quadros, cada um um .cur inteiro, numa lista "fram".
fn ler_ani(b: &[u8], tipo: Tipo, op: &Opcoes) -> Option<Animacao> {
    let mut quadros = Vec::new();
    let (mut taxa, mut passos_cab) = (6u32, 0usize);
    let (mut taxas, mut ordem): (Option<Vec<u32>>, Option<Vec<u32>>) = (None, None);
    let lista = |ini: usize, fim: usize| (ini..fim).step_by(4).map(|k| le32(b, k)).collect::<Vec<u32>>();
    let mut i = 12;
    while i + 8 <= b.len() {
        let id = &b[i..i + 4];
        let tam = le32(b, i + 4) as usize;
        let ini = i + 8;
        let fim = ini.saturating_add(tam).min(b.len());
        match id {
            b"anih" => {
                passos_cab = le32(b, ini + 8) as usize;
                taxa = le32(b, ini + 28).max(1);
            }
            b"rate" => taxas = Some(lista(ini, fim)),
            b"seq " => ordem = Some(lista(ini, fim)),
            b"LIST" if b.get(ini..ini + 4) == Some(b"fram") => {
                let mut j = ini + 4;
                while j + 8 <= fim {
                    let t = le32(b, j + 4) as usize;
                    if &b[j..j + 4] == b"icon" {
                        if let Some(q) = b.get(j + 8..(j + 8).saturating_add(t)).and_then(|d| ler_imagem(d, tipo, op)) {
                            quadros.push(q);
                        }
                    }
                    j = j + 8 + t + (t & 1);
                }
            }
            _ => {}
        }
        i = ini + tam + (tam & 1);
    }
    if quadros.is_empty() {
        return None;
    }
    let n = ordem.as_ref().map_or(if passos_cab > 0 { passos_cab } else { quadros.len() }, Vec::len);
    let passos: Vec<(usize, f32)> = (0..n)
        .map(|k| {
            let q = ordem.as_ref().and_then(|o| o.get(k)).map_or(k, |&x| x as usize).min(quadros.len() - 1);
            let jiffies = taxas.as_ref().and_then(|t| t.get(k)).copied().unwrap_or(taxa).max(1);
            (q, jiffies as f32 / 60.0)
        })
        .collect();
    let total = passos.iter().map(|p| p.1).sum::<f32>().max(0.001);
    Some(Animacao { quadros, passos, total })
}

/// Pede ao Windows para montar o ícone (aceita os bitmaps antigos e PNG) e o desenha
/// sobre preto e sobre branco: a diferença dá a transparência de cada pixel.
fn decodificar(dados: &[u8], w: usize, h: usize) -> Option<Vec<Cor>> {
    if w == 0 || h == 0 || w > 1024 || h > 1024 {
        return None;
    }
    unsafe {
        let icone = CreateIconFromResourceEx(dados.as_ptr(), dados.len() as u32, 1, 0x0003_0000, w as i32, h as i32, 0);
        if icone == 0 {
            return None;
        }
        let preto = desenhar(icone, w, h, 0x0000_0000);
        let branco = desenhar(icone, w, h, 0x00FF_FFFF);
        DestroyIcon(icone);
        let (preto, branco) = (preto?, branco?);
        let canal = |c: u32, k: u32| ((c >> k) & 255) as f32 / 255.0;
        Some(
            preto
                .iter()
                .zip(&branco)
                .map(|(&p, &b)| {
                    let (r, g, bl) = (canal(p, 16), canal(p, 8), canal(p, 0));
                    let dif = (canal(b, 16) - r + canal(b, 8) - g + canal(b, 0) - bl) / 3.0;
                    if dif < -0.5 {
                        // pixel que inverte o fundo (cursores monocromáticos antigos): vira preto
                        return [0.0, 0.0, 0.0, 1.0];
                    }
                    let a = (1.0 - dif).clamp(0.0, 1.0);
                    [r.min(a), g.min(a), bl.min(a), a]
                })
                .collect(),
        )
    }
}

unsafe fn desenhar(icone: HANDLE, w: usize, h: usize, fundo: u32) -> Option<Vec<u32>> {
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER { biSize: 40, biWidth: w as i32, biHeight: -(h as i32), biPlanes: 1, biBitCount: 32, ..Default::default() },
        bmiColors: [0],
    };
    let mut bits: *mut c_void = null_mut();
    let dib = CreateDIBSection(0, &bmi, 0, &mut bits, 0, 0);
    if dib == 0 || bits.is_null() {
        return None;
    }
    let px = std::slice::from_raw_parts_mut(bits as *mut u32, w * h);
    px.fill(fundo);
    let dc = CreateCompatibleDC(0);
    let antigo = SelectObject(dc, dib);
    let ok = DrawIconEx(dc, 0, 0, icone, w as i32, h as i32, 0, 0, DI_NORMAL) != 0;
    let copia = px.to_vec();
    SelectObject(dc, antigo);
    DeleteDC(dc);
    DeleteObject(dib);
    ok.then_some(copia)
}

// ---------- Prévia para a tela de ajustes ----------

/// Seta, mão, texto e espera lado a lado, em PNG com fundo transparente, `escala` vezes o
/// tamanho normal (2 na tela de ajustes, para telas nítidas).
pub fn previa_png(id: &str, cfg: &Config, escala: f32) -> Vec<u8> {
    // "cores" não usa o guardado: a prévia vem com as cores que ainda estão sendo escolhidas
    let tema = if id == cfg.tema && id != "cores" { obter(cfg) } else { Arc::new(carregar(id, cfg)) };
    let celula = (32.0 * escala).round() as usize;
    let tipos = [Tipo::Seta, Tipo::Mao, Tipo::Texto, Tipo::Espera];
    let (w, h) = (celula * tipos.len(), celula);
    let mut acum = vec![[0.0f32; 4]; w * h];
    let mut sp = Sprite::novo();
    for (k, &tipo) in tipos.iter().enumerate() {
        let mut s = escala;
        // imagens grandes (esquemas "extra grande") são reduzidas para caber
        for _ in 0..2 {
            sp.renderizar(&[Camada { tipo, alfa: 1.0, escala: s }], &tema, cfg.sombra, 0.35);
            let (a, b) = caixa(&sp);
            let lado = (b.0 - a.0).max(b.1 - a.1) as f32;
            if lado <= (celula - 6) as f32 {
                break;
            }
            s *= (celula - 6) as f32 / lado;
        }
        let (a, b) = caixa(&sp);
        let dx = (k * celula) as i32 + (celula as i32 - (b.0 - a.0)) / 2 - a.0;
        let dy = (celula as i32 - (b.1 - a.1)) / 2 - a.1;
        for j in 0..sp.h {
            for i in 0..sp.w {
                let (x, y) = (i as i32 + dx, j as i32 + dy);
                if x < (k * celula) as i32 || x >= ((k + 1) * celula) as i32 || y < 0 || y >= h as i32 {
                    continue;
                }
                acum[y as usize * w + x as usize] = sp.px[j * sp.w + i];
            }
        }
    }
    let mut rgba = vec![0u8; w * h * 4];
    for (i, c) in acum.iter().enumerate() {
        let a = c[3].clamp(0.0, 1.0);
        if a > 0.0 {
            for q in 0..3 {
                rgba[i * 4 + q] = ((c[q] / a).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
        }
        rgba[i * 4 + 3] = (a * 255.0 + 0.5) as u8;
    }
    crate::png::codificar(w, h, &rgba)
}

/// Retângulo do que aparece no sprite: ((x0, y0), (x1, y1)), em pixels.
fn caixa(sp: &Sprite) -> ((i32, i32), (i32, i32)) {
    let (mut a, mut b) = ((i32::MAX, i32::MAX), (i32::MIN, i32::MIN));
    for j in 0..sp.h {
        for i in 0..sp.w {
            if sp.px[j * sp.w + i][3] > 0.03 {
                a = (a.0.min(i as i32), a.1.min(j as i32));
                b = (b.0.max(i as i32 + 1), b.1.max(j as i32 + 1));
            }
        }
    }
    if a.0 > b.0 {
        return ((0, 0), (1, 1));
    }
    (a, b)
}
