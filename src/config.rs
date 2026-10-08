// Configuração lida de flowcursor.ini. O arquivo é relido sozinho quando muda,
// e a tela de ajustes grava nele.
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub estilo: Estilo,
    pub tamanho: f32,
    pub responsividade: f32,
    pub elasticidade: f32,
    pub antecipacao: f32,
    pub borrao: f32,
    pub rastro: f32,
    pub rastro_intensidade: f32,
    pub rastro_velocidade: f32,
    pub sombra: f32,
    pub clique: bool,
    pub onda_clique: bool,
    pub desativar_tela_cheia: bool,
    pub alternador: bool,
    pub alttab_transparencia: f32,
    pub alttab_fosco: f32,
    pub alttab_cor: f32,
    pub alttab_tamanho: f32,
    pub alttab_painel: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Estilo {
    Apple,
    Claro,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            estilo: Estilo::Apple,
            tamanho: 1.0,
            responsividade: 75.0,
            elasticidade: 10.0,
            antecipacao: 35.0,
            borrao: 60.0,
            rastro: 30.0,
            rastro_intensidade: 55.0,
            rastro_velocidade: 1200.0,
            sombra: 35.0,
            clique: true,
            onda_clique: true,
            desativar_tela_cheia: true,
            alternador: true,
            alttab_transparencia: 85.0,
            alttab_fosco: 50.0,
            alttab_cor: 50.0,
            alttab_tamanho: 60.0,
            alttab_painel: 90.0,
        }
    }
}

/// Caminho do flowcursor.ini e a data da última gravação ou leitura,
/// para o relógio de recarga não reler o que acabou de ser gravado.
pub static CAMINHO: OnceLock<PathBuf> = OnceLock::new();
pub static DATA: Mutex<Option<SystemTime>> = Mutex::new(None);

pub const MODELO: &str = "\
# FlowCursor: configuracoes
# Salve o arquivo e a mudanca vale na hora, sem reabrir o programa.
# Tambem da para ajustar tudo pela tela: clique duplo no icone da bandeja.
# Atalhos: Ctrl+Alt+F9 pausa/retoma, Ctrl+Alt+F10 fecha e devolve o ponteiro normal.

# Aparencia
estilo = apple            # apple (preto com borda branca) ou claro (branco com borda preta)
tamanho = 1.0             # 1.0 = tamanho do ponteiro padrao do Windows
sombra = 35               # 0 a 100

# Movimento
responsividade = 75       # 0 a 100. Mais alto = o ponteiro desenhado cola no real (100 = sem mola)
elasticidade = 10         # 0 a 100. Mais alto = passa um pouco do ponto e volta, como mola
antecipacao = 35          # 0 a 100. Compensa o atraso quando o movimento e continuo

# Efeitos
borrao = 60               # 0 a 100. Borrao de movimento: proporcional a velocidade
rastro = 30               # 0 a 100. Comprimento do rastro em movimentos rapidos (0 desliga)
rastro_intensidade = 55   # 0 a 100. Opacidade do rastro
rastro_velocidade = 1200  # pixels por segundo a partir dos quais o rastro aparece
clique = sim              # o ponteiro encolhe um pouco ao clicar
onda_clique = sim         # onda que se abre no ponto do clique

# Compatibilidade
desativar_tela_cheia = sim  # jogos e videos em tela cheia usam o ponteiro normal

# Alt+Tab
alternador = sim          # Alt+Tab do FlowCursor: miniaturas ao vivo agrupadas por aplicativo
alttab_transparencia = 85 # 0 a 100. Quanto do fundo aparece atras do vidro do painel
alttab_fosco = 50         # 0 a 100. 0 vidro limpo (sem desfoque), 100 bem fosco e leitoso
alttab_cor = 50           # 0 a 100. Cor do icone no topo de cada cartao (0 desliga)
alttab_tamanho = 60       # 0 a 100. Tamanho das miniaturas; ate 20 mostra uma lista com o nome inteiro
alttab_painel = 90        # 40 a 100. Quanto da tela o painel pode ocupar (%)
";

fn sim_nao(v: &str) -> Option<bool> {
    match v.to_lowercase().as_str() {
        "sim" | "s" | "true" | "1" | "ligado" => Some(true),
        "nao" | "não" | "n" | "false" | "0" | "desligado" => Some(false),
        _ => None,
    }
}

/// Interpreta texto no formato do .ini. Chaves desconhecidas ou valores
/// inválidos ficam no padrão e voltam como avisos.
pub fn analisar(texto: &str) -> (Config, Vec<String>) {
    let mut c = Config::default();
    let mut avisos = Vec::new();
    for (n, linha) in texto.lines().enumerate() {
        let linha = linha.split('#').next().unwrap_or("").trim();
        if linha.is_empty() {
            continue;
        }
        let Some((k, v)) = linha.split_once('=') else {
            avisos.push(format!("linha {}: falta o '='", n + 1));
            continue;
        };
        let (k, v) = (k.trim().to_lowercase(), v.trim());
        let num = |faixa: (f32, f32)| v.replace(',', ".").parse::<f32>().ok().map(|x| x.clamp(faixa.0, faixa.1));
        let ok = match k.as_str() {
            "estilo" => match v.to_lowercase().as_str() {
                "apple" | "mac" | "macos" => Some(c.estilo = Estilo::Apple),
                "claro" | "windows" | "fluent" => Some(c.estilo = Estilo::Claro),
                _ => None,
            },
            "tamanho" => num((0.5, 4.0)).map(|x| c.tamanho = x),
            "responsividade" => num((0.0, 100.0)).map(|x| c.responsividade = x),
            "elasticidade" => num((0.0, 100.0)).map(|x| c.elasticidade = x),
            "antecipacao" | "antecipação" => num((0.0, 100.0)).map(|x| c.antecipacao = x),
            "borrao" | "borrão" => num((0.0, 100.0)).map(|x| c.borrao = x),
            "rastro" => num((0.0, 100.0)).map(|x| c.rastro = x),
            "rastro_intensidade" => num((0.0, 100.0)).map(|x| c.rastro_intensidade = x),
            "rastro_velocidade" => num((100.0, 20000.0)).map(|x| c.rastro_velocidade = x),
            "sombra" => num((0.0, 100.0)).map(|x| c.sombra = x),
            "clique" => sim_nao(v).map(|x| c.clique = x),
            "onda_clique" => sim_nao(v).map(|x| c.onda_clique = x),
            "desativar_tela_cheia" => sim_nao(v).map(|x| c.desativar_tela_cheia = x),
            "alternador" => sim_nao(v).map(|x| c.alternador = x),
            "alttab_transparencia" => num((0.0, 100.0)).map(|x| c.alttab_transparencia = x),
            "alttab_fosco" => num((0.0, 100.0)).map(|x| c.alttab_fosco = x),
            "alttab_cor" => num((0.0, 100.0)).map(|x| c.alttab_cor = x),
            "alttab_tamanho" => num((0.0, 100.0)).map(|x| c.alttab_tamanho = x),
            "alttab_painel" => num((40.0, 100.0)).map(|x| c.alttab_painel = x),
            _ => {
                avisos.push(format!("linha {}: chave desconhecida '{k}'", n + 1));
                continue;
            }
        };
        if ok.is_none() {
            avisos.push(format!("linha {}: valor inválido para '{k}': '{v}'", n + 1));
        }
    }
    (c, avisos)
}

pub fn carregar(caminho: &Path) -> (Config, Vec<String>) {
    // Tolera arquivo salvo em ANSI pelo Bloco de Notas e o BOM do UTF-8.
    match std::fs::read(caminho) {
        Ok(b) => analisar(String::from_utf8_lossy(&b).trim_start_matches('\u{feff}')),
        Err(e) => (Config::default(), vec![format!("não consegui ler {}: {e}", caminho.display())]),
    }
}

fn numero(x: f32) -> String {
    if (x - x.round()).abs() < 1e-4 {
        format!("{}", x.round() as i64)
    } else {
        format!("{:.2}", x).trim_end_matches('0').to_string()
    }
}

/// Valores na forma escrita no .ini, na ordem do modelo.
pub fn pares(c: &Config) -> Vec<(&'static str, String)> {
    let sn = |b: bool| if b { "sim" } else { "nao" }.to_string();
    vec![
        ("estilo", if c.estilo == Estilo::Apple { "apple" } else { "claro" }.to_string()),
        ("tamanho", numero(c.tamanho)),
        ("sombra", numero(c.sombra)),
        ("responsividade", numero(c.responsividade)),
        ("elasticidade", numero(c.elasticidade)),
        ("antecipacao", numero(c.antecipacao)),
        ("borrao", numero(c.borrao)),
        ("rastro", numero(c.rastro)),
        ("rastro_intensidade", numero(c.rastro_intensidade)),
        ("rastro_velocidade", numero(c.rastro_velocidade)),
        ("clique", sn(c.clique)),
        ("onda_clique", sn(c.onda_clique)),
        ("desativar_tela_cheia", sn(c.desativar_tela_cheia)),
        ("alternador", sn(c.alternador)),
        ("alttab_transparencia", numero(c.alttab_transparencia)),
        ("alttab_fosco", numero(c.alttab_fosco)),
        ("alttab_cor", numero(c.alttab_cor)),
        ("alttab_tamanho", numero(c.alttab_tamanho)),
        ("alttab_painel", numero(c.alttab_painel)),
    ]
}

/// Texto do .ini com os comentários do modelo e os valores de `c`.
pub fn para_ini(c: &Config) -> String {
    let valores = pares(c);
    let mut saida = String::new();
    for linha in MODELO.lines() {
        let codigo = linha.split('#').next().unwrap_or("");
        let chave = codigo.split('=').next().unwrap_or("").trim();
        match valores.iter().find(|(k, _)| *k == chave).filter(|_| codigo.contains('=')) {
            Some((k, v)) => {
                let novo = format!("{k} = {v}");
                let largura = codigo.len().max(novo.len() + 1);
                saida.push_str(&format!("{:<largura$}{}", novo, &linha[codigo.len()..]));
            }
            None => saida.push_str(linha),
        }
        saida.push_str("\r\n");
    }
    saida
}

pub fn para_json(c: &Config) -> String {
    let campos: Vec<String> = pares(c)
        .into_iter()
        .map(|(k, v)| match v.as_str() {
            "sim" => format!("\"{k}\":true"),
            "nao" => format!("\"{k}\":false"),
            _ if v.parse::<f64>().is_ok() => format!("\"{k}\":{v}"),
            _ => format!("\"{k}\":\"{v}\""),
        })
        .collect();
    format!("{{{}}}", campos.join(","))
}

/// Grava a configuração e marca a data, para não ser relida como mudança externa.
pub fn salvar(c: &Config) -> std::io::Result<()> {
    let caminho = CAMINHO.get().ok_or_else(|| std::io::Error::other("caminho da configuração não definido"))?;
    std::fs::write(caminho, para_ini(c))?;
    *DATA.lock().unwrap_or_else(|e| e.into_inner()) = std::fs::metadata(caminho).and_then(|m| m.modified()).ok();
    Ok(())
}

/// Procura flowcursor.ini na pasta do .exe e nas pastas acima (na compilação
/// o .exe fica em target\release). Se não achar, cria o modelo ao lado do .exe.
pub fn localizar() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("flowcursor.exe"));
    let pasta_exe = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut pasta = Some(pasta_exe.as_path());
    for _ in 0..4 {
        let Some(p) = pasta else { break };
        let candidato = p.join("flowcursor.ini");
        if candidato.exists() {
            return candidato;
        }
        pasta = p.parent();
    }
    let novo = pasta_exe.join("flowcursor.ini");
    let _ = std::fs::write(&novo, MODELO);
    novo
}
