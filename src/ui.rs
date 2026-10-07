// Tela de ajustes: uma página servida só para este PC (127.0.0.1, com chave
// aleatória) e aberta numa janela própria do Edge, sem barra de endereço.
// Cada mudança é aplicada na hora e gravada no flowcursor.ini.
use crate::config::{self, Config};
use crate::ffi::*;
use crate::laco;
use crate::reg;
use crate::sistema;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Child;
use std::ptr::null;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const PAGINA: &str = include_str!("ui/index.html");
const TITULO_JANELA: &str = "FlowCursor";
const LIMITE_PEDIDO: usize = 64 * 1024;

static SERVIDOR: OnceLock<(u16, String)> = OnceLock::new();
static JANELA: Mutex<Option<Child>> = Mutex::new(None);

fn chave_aleatoria() -> String {
    // RandomState é semeado pelo sistema operacional a cada instância.
    let mut s = String::new();
    for i in 0..2u64 {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(i ^ std::process::id() as u64);
        s.push_str(&format!("{:016x}", h.finish()));
    }
    s
}

fn iniciar_servidor() -> Option<(u16, String)> {
    if let Some(s) = SERVIDOR.get() {
        return Some(s.clone());
    }
    let ouvinte = TcpListener::bind("127.0.0.1:0").map_err(|e| reg!("tela de ajustes: não consegui abrir a porta: {e}")).ok()?;
    let porta = ouvinte.local_addr().ok()?.port();
    let chave = chave_aleatoria();
    let _ = SERVIDOR.set((porta, chave.clone()));
    reg!("tela de ajustes servida em 127.0.0.1:{porta}");
    std::thread::spawn(move || {
        for conexao in ouvinte.incoming().flatten() {
            let chave = chave.clone();
            // uma linha de execução por conexão: o Edge abre conexões que ficam paradas
            std::thread::spawn(move || {
                let _ = atender(conexao, porta, &chave);
            });
        }
    });
    SERVIDOR.get().cloned()
}

struct Pedido {
    metodo: String,
    caminho: String,
    consulta: String,
    host: String,
    chave: String,
    corpo: String,
}

fn ler_pedido(s: &mut TcpStream) -> Option<Pedido> {
    s.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut buf = Vec::new();
    let mut bloco = [0u8; 4096];
    let fim_cabecalho = loop {
        let n = s.read(&mut bloco).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&bloco[..n]);
        if let Some(p) = buf.windows(4).position(|j| j == b"\r\n\r\n") {
            break p + 4;
        }
        if buf.len() > LIMITE_PEDIDO {
            return None;
        }
    };
    let cabecalho = String::from_utf8_lossy(&buf[..fim_cabecalho]).into_owned();
    let mut linhas = cabecalho.lines();
    let mut primeira = linhas.next()?.split_whitespace();
    let metodo = primeira.next()?.to_string();
    let alvo = primeira.next()?.to_string();
    let (caminho, consulta) = alvo.split_once('?').map(|(a, b)| (a.to_string(), b.to_string())).unwrap_or((alvo, String::new()));
    let (mut host, mut chave, mut tamanho) = (String::new(), String::new(), 0usize);
    for l in linhas {
        if let Some((k, v)) = l.split_once(':') {
            match k.trim().to_lowercase().as_str() {
                "host" => host = v.trim().to_string(),
                "x-token" => chave = v.trim().to_string(),
                "content-length" => tamanho = v.trim().parse().unwrap_or(0),
                _ => {}
            }
        }
    }
    if tamanho > LIMITE_PEDIDO {
        return None;
    }
    let mut corpo = buf[fim_cabecalho..].to_vec();
    while corpo.len() < tamanho {
        let n = s.read(&mut bloco).ok()?;
        if n == 0 {
            break;
        }
        corpo.extend_from_slice(&bloco[..n]);
    }
    corpo.truncate(tamanho);
    Some(Pedido { metodo, caminho, consulta, host, chave, corpo: String::from_utf8_lossy(&corpo).into_owned() })
}

fn responder(s: &mut TcpStream, codigo: &str, tipo: &str, corpo: &str) -> std::io::Result<()> {
    let cab = format!(
        "HTTP/1.1 {codigo}\r\nContent-Type: {tipo}\r\nContent-Length: {}\r\nCache-Control: no-store\r\n\
         X-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        corpo.len()
    );
    s.write_all(cab.as_bytes())?;
    s.write_all(corpo.as_bytes())
}

fn estado_json() -> String {
    format!(
        "{{\"ativo\":{},\"iniciar\":{},\"versao\":\"{}\",\"config\":{},\"padrao\":{}}}",
        !laco::PAUSADO.load(Relaxed),
        sistema::inicia_com_windows(),
        env!("CARGO_PKG_VERSION"),
        config::para_json(&laco::config_atual()),
        config::para_json(&Config::default())
    )
}

fn aplicar(cfg: Config) -> Result<(), String> {
    config::salvar(&cfg).map_err(|e| format!("não consegui gravar o flowcursor.ini: {e}"))?;
    laco::publicar_config(cfg);
    Ok(())
}

fn atender(mut s: TcpStream, porta: u16, chave: &str) -> std::io::Result<()> {
    let Some(p) = ler_pedido(&mut s) else { return Ok(()) };
    // Só aceita o endereço local exato (protege contra páginas que tentem se passar por ele).
    if p.host != format!("127.0.0.1:{porta}") {
        return responder(&mut s, "403 Forbidden", "text/plain", "endereço não permitido");
    }
    let chave_ok = p.chave == chave || p.consulta.split('&').any(|par| par == format!("t={chave}"));
    // o ícone é pedido pelo próprio Edge, sem a chave; não tem nada de sensível
    if !chave_ok && p.caminho != "/icone.svg" {
        return responder(&mut s, "403 Forbidden", "text/plain", "chave inválida");
    }
    let json = "application/json; charset=utf-8";
    match (p.metodo.as_str(), p.caminho.as_str()) {
        ("GET", "/") => responder(&mut s, "200 OK", "text/html; charset=utf-8", PAGINA),
        ("GET", "/icone.svg") => responder(&mut s, "200 OK", "image/svg+xml", ICONE_SVG),
        ("GET", "/api/estado") => responder(&mut s, "200 OK", json, &estado_json()),
        ("POST", "/api/config") => {
            let (cfg, avisos) = config::analisar(&p.corpo);
            if !avisos.is_empty() {
                reg!("tela de ajustes mandou valores inválidos: {}", avisos.join("; "));
            }
            match aplicar(cfg) {
                Ok(()) => responder(&mut s, "200 OK", json, &estado_json()),
                Err(e) => responder(&mut s, "500 Internal Server Error", "text/plain; charset=utf-8", &e),
            }
        }
        ("POST", "/api/padrao") => {
            reg!("tela de ajustes: padrões restaurados");
            match aplicar(Config::default()) {
                Ok(()) => responder(&mut s, "200 OK", json, &estado_json()),
                Err(e) => responder(&mut s, "500 Internal Server Error", "text/plain; charset=utf-8", &e),
            }
        }
        ("POST", "/api/ativo") => {
            laco::PAUSADO.store(p.corpo.trim() != "sim", Relaxed);
            reg!("tela de ajustes: {}", if p.corpo.trim() == "sim" { "retomado" } else { "pausado" });
            responder(&mut s, "200 OK", json, &estado_json())
        }
        ("POST", "/api/iniciar") => {
            let ligar = p.corpo.trim() == "sim";
            let exe = std::env::current_exe().unwrap_or_default();
            if !sistema::definir_inicio_com_windows(ligar, &exe) {
                reg!("não consegui mudar o início com o Windows");
            } else {
                reg!("iniciar com o Windows: {}", if ligar { "ligado" } else { "desligado" });
            }
            responder(&mut s, "200 OK", json, &estado_json())
        }
        ("POST", "/api/registro") => {
            if let Some(c) = config::CAMINHO.get() {
                abrir_arquivo(&c.with_file_name("flowcursor.log"));
            }
            responder(&mut s, "200 OK", json, "{}")
        }
        _ => responder(&mut s, "404 Not Found", "text/plain", "não encontrado"),
    }
}

pub fn abrir_arquivo(c: &std::path::Path) {
    unsafe {
        ShellExecuteW(0, w("open").as_ptr(), w(&c.to_string_lossy()).as_ptr(), null(), null(), SW_SHOWNORMAL);
    }
}

fn caminho_edge() -> Option<PathBuf> {
    [r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe", r"C:\Program Files\Microsoft\Edge\Application\msedge.exe"]
        .iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
}

/// Abre a tela de ajustes, ou traz para a frente se já estiver aberta.
pub fn abrir_janela() {
    unsafe {
        let existente = FindWindowW(w("Chrome_WidgetWin_1").as_ptr(), w(TITULO_JANELA).as_ptr());
        if existente != 0 {
            if IsIconic(existente) != 0 {
                ShowWindow(existente, SW_RESTORE);
            }
            SetForegroundWindow(existente);
            return;
        }
    }
    let Some((porta, chave)) = iniciar_servidor() else { return };
    let url = format!("http://127.0.0.1:{porta}/?t={chave}");
    match caminho_edge() {
        Some(edge) => {
            // Perfil próprio do Edge: a janela não se mistura com o navegador do dia a dia.
            let perfil = config::CAMINHO.get().and_then(|c| c.parent().map(|p| p.join(".janela"))).unwrap_or_else(|| PathBuf::from(".janela"));
            let filho = std::process::Command::new(edge)
                .arg(format!("--app={url}"))
                .arg(format!("--user-data-dir={}", perfil.display()))
                .arg("--window-size=520,920")
                .arg("--no-first-run")
                .arg("--no-default-browser-check")
                .arg("--disable-features=Translate,msEdgeSidebarV2")
                .spawn();
            match filho {
                Ok(f) => *JANELA.lock().unwrap_or_else(|e| e.into_inner()) = Some(f),
                Err(e) => reg!("não consegui abrir o Edge: {e}"),
            }
        }
        None => unsafe {
            ShellExecuteW(0, w("open").as_ptr(), w(&url).as_ptr(), null(), null(), SW_SHOWNORMAL);
        },
    }
}

/// Fecha a tela de ajustes junto com o FlowCursor (sem o servidor ela não funcionaria).
pub fn fechar_janela() {
    if let Some(mut f) = JANELA.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = f.kill();
        let _ = f.wait();
    }
}

pub const ICONE_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#5ac8fa"/><stop offset=".55" stop-color="#5e5ce6"/><stop offset="1" stop-color="#bf5af2"/></linearGradient></defs>
<rect x="2" y="2" width="60" height="60" rx="15" fill="url(#g)"/>
<rect x="2" y="2" width="60" height="30" rx="15" fill="#fff" opacity=".14"/>
<polygon transform="translate(22 14) scale(1.65)" points="0.5,0.5 0.5,17.5 4.5,13.8 7.3,20.1 9.9,19 7.2,12.9 12.5,12.9" fill="#0b0b0d" stroke="#fff" stroke-width="2.4" stroke-linejoin="round" paint-order="stroke"/>
</svg>"##;
