# FlowCursor

Deixa o ponteiro do mouse com movimento suave (mola), borrão de movimento e rastro. Também troca o **Alt+Tab** do Windows por um painel de vidro com miniaturas ao vivo das janelas.

O ponteiro de verdade continua fazendo os cliques; só o desenho muda. Por isso a precisão e o tempo do clique continuam iguais.

## Como abrir

- Atalho **FlowCursor** na área de trabalho. Abre o programa e a tela de ajustes; se já estiver aberto, só traz os ajustes para a frente.
- Ele **liga sozinho com o Windows**. Dá para desligar isso nos ajustes.
- Na **bandeja** (perto do relógio):
  - clique duplo abre os ajustes;
  - clique direito mostra: ajustes, ativo, arquivo de configuração, registro e sair.

## Atalhos

| Tecla | O que faz |
|---|---|
| **Ctrl+Alt+F9** | Pausa e retoma o FlowCursor |
| **Ctrl+Alt+F10** | Fecha e devolve o ponteiro normal |
| **Alt+Tab** | Abre o painel de janelas (ver abaixo) |

### No painel do Alt+Tab

| Tecla | O que faz |
|---|---|
| **Tab / Shift+Tab** | Anda na ordem de uso |
| **Setas** | Anda pela tela |
| **Enter** ou clique | Abre a janela |
| **Del** ou botão do meio | Fecha a janela |
| **Esc**, clique fora ou tecla Windows | Cancela |
| **Digitar** | Filtra pelo título ou pelo nome do programa |

- Não precisa segurar o Alt: se soltar com o painel aberto, ele fica lá para você usar o mouse, as setas ou a busca. Fecha sozinho depois de 30 s parado.
- Um Alt+Tab rápido troca para a janela anterior sem mostrar o painel, como no Windows.
- Parar o mouse sobre uma miniatura mostra a janela inteira em tamanho grande.

## Ajustes

Pela tela de ajustes, as mudanças valem na hora e há uma área de teste com todos os ponteiros:
- **Estilo:** preto com borda branca, ou branco com borda preta.
- **Tamanho**.
- **Mola:**
  - responsividade: quanto o desenho gruda no ponteiro;
  - elasticidade: quanto passa do ponto e volta;
  - antecipação.
- **Borrão** e **rastro:** comprimento, intensidade e a velocidade em que o rastro aparece.
- **Efeito de clique:** o ponteiro encolhe e solta uma onda.
- **Desativar em tela cheia:** jogos e vídeos usam o ponteiro normal.
- **Alt+Tab:** vidro, cor dos cartões, tamanho das miniaturas, tamanho do painel e o botão **Ver o Alt+Tab** para testar.

Tudo isso fica em `flowcursor.ini`, nesta pasta. Dá para editar à mão; salvou, vale na hora.

## Se o ponteiro ficar estranho ou travado

O ponteiro normal volta sozinho se algo der errado: um processo guardião devolve os cursores se o FlowCursor travar ou for fechado à força. Nada é gravado nas configurações do Windows.

Para devolver na mão, use qualquer uma destas:
- o botão **↻**, no topo dos ajustes (fecha, devolve o ponteiro e abre de novo);
- **Ctrl+Alt+F10**;
- `FlowCursor.exe --restaurar`;
- reiniciar o PC ou sair e entrar na conta.

## Como funciona

O FlowCursor esconde o desenho do ponteiro do Windows e desenha o próprio numa camada transparente por cima de tudo, seguindo o ponteiro real com física de mola. É escrito em Rust.

## Para mexer no programa

- `compilar.ps1` compila e abre a versão nova.
- `empacotar.ps1` gera o instalador.
- O código está em `src\`.
- O `README.md` tem os detalhes técnicos.
- Problemas ou ideias: botão na tela de ajustes, ou em github.com/atilacarpes96/FlowCursor/issues.
