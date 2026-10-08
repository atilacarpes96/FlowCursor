// Atualização pelo GitHub: a tela de ajustes pergunta qual é a última versão
// publicada (Releases do repositório), baixa o instalador dela e roda em modo
// silencioso. O instalador fecha este FlowCursor, troca o .exe (os ajustes ficam)
// e abre a versão nova com a tela de ajustes.
use crate::ffi::*;
use crate::reg;
use std::ffi::c_void;
use std::ptr::{null, null_mut};

pub const REPOSITORIO: &str = "atilacarpes96/FlowCursor";
const VERSAO: &str = env!("CARGO_PKG_VERSION");

struct Alca(HANDLE);

impl Drop for Alca {
    fn drop(&mut self) {
        if self.0 != 0 {
            unsafe { WinHttpCloseHandle(self.0) };
        }
    }
}

fn erro(o_que: &str) -> String {
    format!("{o_que} (erro {})", std::io::Error::last_os_error().raw_os_error().unwrap_or(0))
}

/// GET de uma URL https. Os redirecionamentos (o GitHub manda o download para
/// outro servidor) são seguidos pelo próprio WinHTTP.
fn baixar(url: &str, aceitar: &str) -> Result<(u32, Vec<u8>), String> {
    let resto = url.strip_prefix("https://").ok_or("só https")?;
    let (servidor, caminho) = resto.split_once('/').map(|(s, c)| (s, format!("/{c}"))).unwrap_or((resto, "/".into()));
    unsafe {
        let agente = w(&format!("FlowCursor/{VERSAO}"));
        let mut sessao = Alca(WinHttpOpen(agente.as_ptr(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, null(), null(), 0));
        if sessao.0 == 0 {
            // Windows antigo sem o proxy automático
            sessao = Alca(WinHttpOpen(agente.as_ptr(), WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, null(), null(), 0));
        }
        if sessao.0 == 0 {
            return Err(erro("não consegui iniciar a conexão"));
        }
        WinHttpSetTimeouts(sessao.0, 10_000, 10_000, 15_000, 30_000);
        let conexao = Alca(WinHttpConnect(sessao.0, w(servidor).as_ptr(), 443, 0));
        if conexao.0 == 0 {
            return Err(erro("não consegui conectar ao GitHub"));
        }
        let pedido = Alca(WinHttpOpenRequest(conexao.0, w("GET").as_ptr(), w(&caminho).as_ptr(), null(), null(), null(), WINHTTP_FLAG_SECURE));
        if pedido.0 == 0 {
            return Err(erro("não consegui preparar o pedido"));
        }
        let cabecalhos = w(&format!("Accept: {aceitar}\r\n"));
        if WinHttpSendRequest(pedido.0, cabecalhos.as_ptr(), u32::MAX, null(), 0, 0, 0) == 0 || WinHttpReceiveResponse(pedido.0, null_mut()) == 0 {
            return Err(erro("sem resposta do GitHub (sem internet?)"));
        }
        let mut status = 0u32;
        let mut tam = 4u32;
        WinHttpQueryHeaders(pedido.0, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, null(), (&mut status as *mut u32).cast::<c_void>(), &mut tam, null_mut());
        let mut dados = Vec::new();
        let mut bloco = vec![0u8; 64 * 1024];
        loop {
            let mut lidos = 0u32;
            if WinHttpReadData(pedido.0, bloco.as_mut_ptr().cast(), bloco.len() as u32, &mut lidos) == 0 {
                return Err(erro("a conexão caiu no meio do download"));
            }
            if lidos == 0 {
                break;
            }
            dados.extend_from_slice(&bloco[..lidos as usize]);
            if dados.len() > 64 * 1024 * 1024 {
                return Err("resposta grande demais".into());
            }
        }
        Ok((status, dados))
    }
}

/// Valor de texto de uma chave do JSON (o primeiro que aparecer a partir de `de`).
fn texto_json(json: &str, chave: &str, de: usize) -> Option<(String, usize)> {
    let marca = format!("\"{chave}\"");
    let i = de + json[de..].find(&marca)? + marca.len();
    let resto = &json[i..];
    let aspas = resto.find('"')?;
    if !resto[..aspas].trim().trim_start_matches(':').trim().is_empty() {
        return None;
    }
    let inicio = i + aspas + 1;
    let mut valor = String::new();
    let mut escape = false;
    for (k, c) in json[inicio..].char_indices() {
        match (escape, c) {
            (true, 'n') => {
                valor.push('\n');
                escape = false;
            }
            (true, c) => {
                valor.push(c);
                escape = false;
            }
            (false, '\\') => escape = true,
            (false, '"') => return Some((valor, inicio + k + 1)),
            (false, c) => valor.push(c),
        }
    }
    None
}

fn numeros(v: &str) -> Vec<u32> {
    v.trim().trim_start_matches(['v', 'V']).split('.').map(|p| p.chars().take_while(char::is_ascii_digit).collect::<String>().parse().unwrap_or(0)).collect()
}

/// `a` é mais nova que `b`?
pub fn mais_nova(a: &str, b: &str) -> bool {
    let (mut x, mut y) = (numeros(a), numeros(b));
    let n = x.len().max(y.len());
    x.resize(n, 0);
    y.resize(n, 0);
    x > y
}

pub struct Lancamento {
    pub versao: String,
    pub instalador: String,
    pub pagina: String,
}

/// Última versão publicada nos Releases do GitHub.
pub fn ultima() -> Result<Lancamento, String> {
    let (status, dados) = baixar(&format!("https://api.github.com/repos/{REPOSITORIO}/releases/latest"), "application/vnd.github+json")?;
    let json = String::from_utf8_lossy(&dados);
    match status {
        200 => {}
        404 => return Err("ainda não há versão publicada no GitHub".into()),
        403 | 429 => return Err("o GitHub limitou as consultas; tente de novo daqui a pouco".into()),
        s => return Err(format!("o GitHub respondeu {s}")),
    }
    let versao = texto_json(&json, "tag_name", 0).ok_or("resposta do GitHub sem versão")?.0;
    let pagina = texto_json(&json, "html_url", 0).map(|v| v.0).unwrap_or_default();
    let mut de = 0;
    let mut instalador = None;
    while let Some((url, fim)) = texto_json(&json, "browser_download_url", de) {
        if url.to_lowercase().ends_with(".exe") {
            instalador = Some(url);
            break;
        }
        de = fim;
    }
    let instalador = instalador.ok_or("a versão publicada não tem o instalador (.exe)")?;
    Ok(Lancamento { versao, instalador, pagina })
}

/// Baixa o instalador e roda em modo silencioso; ele fecha este FlowCursor e abre o novo.
pub fn instalar(l: &Lancamento) -> Result<(), String> {
    if !l.instalador.starts_with("https://github.com/") && !l.instalador.starts_with("https://objects.githubusercontent.com/") {
        return Err("endereço do instalador inesperado".into());
    }
    reg!("atualização: baixando {} de {}", l.versao, l.instalador);
    let (status, dados) = baixar(&l.instalador, "application/octet-stream")?;
    if status != 200 || dados.len() < 200 * 1024 || !dados.starts_with(b"MZ") {
        return Err(format!("o download não veio inteiro (resposta {status}, {} bytes)", dados.len()));
    }
    // O nome com "Instalador" faz o .exe agir como instalador.
    let destino = std::env::temp_dir().join("FlowCursor-Instalador.exe");
    std::fs::write(&destino, &dados).map_err(|e| format!("não consegui salvar o instalador: {e}"))?;
    reg!("atualização: {} bytes baixados, instalando", dados.len());
    std::process::Command::new(&destino)
        .args(["--silencioso", "--abrir"])
        .current_dir(std::env::temp_dir())
        .spawn()
        .map_err(|e| format!("não consegui abrir o instalador: {e}"))?;
    Ok(())
}
