# FlowCursor

Troca o desenho do ponteiro do Windows por um ponteiro com mola, borrão de
movimento e rastro. O ponteiro real continua fazendo os cliques; só o desenho
muda, então a precisão e o atraso do clique não mudam.

## Usar

- Abrir: atalho **FlowCursor** na área de trabalho. Ele abre o FlowCursor e a tela de
  ajustes; se o FlowCursor já estiver aberto, só traz os ajustes para a frente.
- Na bandeja (perto do relógio): clique duplo abre os ajustes; clique direito tem
  ajustes, ativo, o arquivo de configuração, o registro e sair.
- **Ctrl+Alt+F9**: pausa e retoma.
- **Ctrl+Alt+F10**: fecha e devolve o ponteiro normal.
- Se algo der errado, o ponteiro normal volta sozinho: um processo guardião devolve os
  cursores se o FlowCursor travar ou for fechado à força. Reiniciar o PC ou sair da
  conta também devolve, porque nada é gravado nas configurações do Windows.
- Para devolver na mão: `FlowCursor.exe --restaurar`. Abrir o FlowCursor de novo também
  devolve (tecla Windows, digitar "FlowCursor", Enter), porque ao abrir ele restaura os
  cursores antes de começar. Sair e entrar de novo na conta do Windows também resolve.
- O guardião nasce por um intermediário que fecha na hora, então "Finalizar árvore de
  processos" não o leva junto. E se o FlowCursor for aberto dentro de um grupo de processos
  que encerra tudo junto (o app do Claude faz isso), ele se reabre pelo Explorer, fora do grupo.

## Ajustar

Pela tela de ajustes (mudanças valem na hora, com uma área de teste com todos os
ponteiros), ou direto no `flowcursor.ini`, nesta pasta: salve e a mudança vale na hora.
A tela grava no mesmo arquivo.

| chave | o que faz |
|---|---|
| `estilo` | `apple` (preto com borda branca) ou `claro` (branco com borda preta) |
| `tamanho` | 1.0 = tamanho do ponteiro padrão |
| `responsividade` | quanto o desenho cola no ponteiro real (100 = sem mola) |
| `elasticidade` | quanto passa do ponto e volta |
| `antecipacao` | compensa o atraso da mola em movimento contínuo |
| `borrao` | borrão de movimento |
| `rastro`, `rastro_intensidade`, `rastro_velocidade` | comprimento e opacidade do rastro (cópias da própria seta) e a velocidade a partir da qual ele aparece |
| `clique`, `onda_clique` | o ponteiro encolhe e uma onda se abre ao clicar |
| `desativar_tela_cheia` | jogos e vídeos em tela cheia usam o ponteiro normal |

## Alt+Tab

O FlowCursor troca o Alt+Tab do Windows por um painel flutuante de vidro fosco (o desfoque de
verdade do Windows, que aparece mesmo com o painel sem foco):

- um cartão por aplicativo (o usado por último primeiro) com as janelas em miniaturas quadradas
  ao vivo, todas do mesmo tamanho, com a imagem recortada a partir do canto de cima à esquerda;
- cada cartão leva a cor do ícone do aplicativo (ícone sem cor deixa o cartão neutro), com o
  ícone grande e o nome em destaque no cabeçalho, para achar o app mesmo com miniaturas parecidas;
- parar o mouse sobre uma miniatura abre a prévia da janela inteira por cima das outras (que
  recuam um pouco), com o título embaixo; a prévia não amplia além do tamanho real da janela;
- a seleção é um anel na cor de destaque do Windows, numa camada própria por cima das
  miniaturas (para as vizinhas não o cobrirem);
- não precisa segurar o Alt: soltou com o painel na tela e sem ter escolhido pelo Tab, ele fica
  aberto. Aí dá para usar o mouse, as setas ou digitar a busca. Enter ou clique abre, e Esc, clique
  fora, a tecla Windows ou 30 s parado fecham;
- digitar filtra pelo título ou pelo nome do app;
- Tab e Shift+Tab andam na ordem de uso, setas andam pela tela, Enter abre, Del ou
  botão do meio fecha a janela, Esc cancela;
- um Alt+Tab rápido troca para a janela anterior sem mostrar o painel, como no Windows.

Ctrl+Alt+Tab, Win+Tab e jogos ou vídeos em tela cheia continuam com o Alt+Tab do Windows.
Com uma janela de administrador em primeiro plano (Gerenciador de Tarefas, Editor do Registro,
instaladores) também: o Windows não deixa um programa comum ver o teclado delas.
Desliga nos ajustes, seção "Alt+Tab" (`alternador = nao`). Código em `src/alternador/`.

## Onde o ponteiro normal volta (de propósito)

- Menu Iniciar, pesquisa, Alt+Tab, central de notificações e avisos do Windows:
  ficam numa camada acima de qualquer janela comum, e o desenho ficaria por baixo.
- Janelas que o Windows põe numa camada acima de qualquer "sempre visível", como o
  Gerenciador de Tarefas com "Sempre visível" ligado: o desenho ficaria cortado por baixo
  delas, então sobre elas volta o ponteiro normal.
- Tela de bloqueio, UAC e Ctrl+Alt+Del.
- Janela em tela cheia (se `desativar_tela_cheia = sim`).
- Ponteiros próprios de programas (arrastar arquivo, pincel de editor de imagem,
  alguns jogos): o desenho original continua aparecendo.

O `flowcursor.log` registra cada uma dessas trocas e, a cada 30 s de uso, o desempenho.

## Distribuir

`empacotar.ps1` gera `dist\FlowCursor-Instalador.exe`, o arquivo para mandar. É o próprio
FlowCursor.exe: com "instalador" no nome do arquivo, ele instala.

- Instala sem administrador em `%LOCALAPPDATA%\Programs\FlowCursor`, cria atalhos na
  área de trabalho e no menu Iniciar, registra em Configurações > Aplicativos e,
  se a pessoa deixar marcado, abre junto com o Windows.
- Rodar um instalador mais novo atualiza e mantém os ajustes da pessoa.
- Desinstalar (por Configurações > Aplicativos, ou `FlowCursor.exe --desinstalar`) fecha o
  programa, devolve o ponteiro normal e apaga pasta, atalhos e registro.
- `--silencioso` instala ou desinstala sem perguntar; `--sem-iniciar` não liga o início
  com o Windows.
- O .exe não é assinado digitalmente: na primeira vez o Windows mostra "O Windows protegeu
  o computador". A pessoa clica em "Mais informações" e "Executar assim mesmo". Só uma
  assinatura de código paga (certificado) tira esse aviso.
- O ícone, o manifesto e a versão do .exe saem de `recursos/` (compilados pelo `build.rs`
  com o `rc.exe` do Windows SDK).

## Desenvolvimento

- Rust puro, sem dependências: as funções do Windows estão declaradas em `src/ffi.rs`.
- `compilar.ps1` compila, troca o `FlowCursor.exe` e abre a versão nova.
- `FlowCursor.exe --galeria <pasta>` gera imagens das formas e dos efeitos.
- `FlowCursor.exe --diagnostico <arquivo>` mede a sincronia, testa esconder e devolver
  os cursores (por ~100 ms) e o desempenho do motor.
- `FlowCursor.exe --teste <segundos>` roda e fecha sozinho (substitui uma instância aberta).
- `FlowCursor.exe --ajustes` abre a tela de ajustes; `--icone <arquivo.ico>` gera o ícone.
- A tela de ajustes é servida só para este PC (127.0.0.1, porta aleatória, chave aleatória
  por sessão) e abre numa janela do Edge com perfil próprio em `.janela`.

| arquivo | parte |
|---|---|
| `src/motor.rs` | mola, borrão, rastro, onda do clique e montagem do quadro |
| `src/formas.rs` | desenho vetorial das formas (seta, mão, texto, redimensionar...) |
| `src/sistema.rs` | esconder/devolver cursores, guardião, sincronia com o monitor, vigia |
| `src/overlay.rs` | janela transparente sempre na frente |
| `src/laco.rs` | laço de desenho, um quadro por atualização do monitor |
| `src/ui.rs`, `src/ui/index.html` | tela de ajustes (servidor local e página) |
| `src/main.rs` | início, bandeja, atalhos, recarga da configuração |
