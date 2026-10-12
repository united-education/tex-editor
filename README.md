# tex-editor

Editor visual (estilo Word) para arquivos `.tex`. Você edita o texto
formatado e o editor grava de volta no `.tex`, mudando **só** os caracteres
que você mudou. O resto do arquivo fica idêntico, byte a byte: comentários,
quebras de linha, `\,`, `---`, CRLF.

## Uso

```bash
tex-editor ~/material/livros/biologia/EM1/livro        # uma pasta
tex-editor ~/material/livros/biologia/EM1/livro/capitulos/cap03.tex
```

O comando abre uma janela do Edge (modo app) em `http://localhost:7357`.
Opções: `--no-open` (não abre janela), `TEX_EDITOR_PORT=8000` (outra porta).
O processo fica rodando no terminal. Ctrl+C encerra.

## O que dá para fazer

| Ação | Como |
|---|---|
| Editar texto | clicar e digitar (aspas viram ``…'' sozinhas) |
| Negrito / itálico / sublinhado | Ctrl+B / Ctrl+I / Ctrl+U |
| Sobrescrito / subscrito | Ctrl+Shift+= / Ctrl+= |
| Novo parágrafo ou novo `\item` | Enter |
| Juntar com o anterior | Backspace no início |
| Fórmula | clicar nela (prévia ao vivo) ou Ctrl+M para inserir |
| Título / seção | seletor de estilo na barra |
| Tabelas, figuras, ambientes | clicar abre o código com prévia |
| Desfazer / refazer | Ctrl+Z / Ctrl+Y |
| Ver o fonte ao lado | botão **Fonte** (também permite editar o fonte inteiro) |
| Ver o que mudou | botão **Alterações** |
| Salvar | Ctrl+S |
| Salvar como | Ctrl+Shift+S (cópia ou `.tex` completo, veja abaixo) |
| Códigos ocultos | botão **¶** (comentários, `\label`, `\noindent`…) |

As caixas (`\newtcolorbox`) aparecem com as cores e os títulos do
preâmbulo, que o editor lê dos `.tex` da pasta e das pastas acima.

## Diagramação

### O editor é a página

O editor compila o livro em segundo plano com o **tectonic** e desenha o
texto como ele sai no PDF:

- **Medidas do próprio LaTeX:** largura da mancha, corpo da letra,
  entrelinha, recuo de parágrafo e margens vêm da compilação (o editor
  acrescenta uma linha que escreve essas medidas no `.log`). A fonte é a
  mesma do PDF (Latin Modern, lida do cache do tectonic). As linhas quebram
  quase sempre nos mesmos lugares que no PDF.
- **Fim de cada página:** uma faixa mostra onde a página termina, inclusive
  no meio de um parágrafo, com o espaço em branco que sobrou desenhado em
  escala ("4,1 cm livres no fim da página 6"), a margem, o número da página e
  o começo da seguinte.
- **Onde cada página começa:** o editor lê, com o pdf.js, a primeira linha
  de cada página do PDF e a procura no texto. Enquanto isso não chega, usa o
  SyncTeX. Páginas que só têm figura, ou que vêm de outro arquivo, aparecem
  indicadas na faixa.
- **Figuras flutuantes** (`\imagem`) ficam marcadas com "flutua → pág. N": o
  LaTeX escolhe o lugar delas.
- **Estilos do preâmbulo:** caixas (`\newtcolorbox`), títulos
  (`\titleformat`) e comandos que abrem parágrafo com etiqueta (como
  `\estrategia`, `\resolucao`) aparecem com as cores, tamanhos e etiquetas
  definidos lá.
- **Zoom:** botões − / + e **Ajustar** na barra. O botão **PDF** mostra o PDF
  real ao lado; clicar numa página dele leva ao trecho.

A compilação só acontece quando você manda: botão **▶ Compilar** (ou F5).
Ele fica cinza quando o PDF já corresponde ao texto e verde quando há
mudanças. Cada compilação leva cerca de 8 s num capítulo de 44 páginas. Nada
é gravado no disco para isso: o editor compila a versão da tela numa pasta
temporária.

Passar o mouse num bloco mostra o tamanho dele na página, em cm
("16,0 × 14,2 cm"). Legendas aparecem com o número da figura ("Figura 1.1:"),
lido do `.aux` da última compilação. Texto que não sai no PDF (a descrição
usada para gerar uma imagem, o nome do arquivo da imagem) não aparece.

### Barra de propriedades

Clique num bloco (ou na alça **⠿** à esquerda dele), ou ponha o cursor no
texto, e a barra abaixo da barra principal mostra o bloco e a página onde ele
está:

| Controle | O que faz no LaTeX |
|---|---|
| Letra A− / A+ / tamanho | envolve o bloco em `\begin{small}` … `\end{small}` (ou `footnotesize`, `large`…); "normal" tira |
| Alinhar | `\begin{raggedright}`, `centering`, `raggedleft`; "justificado" tira |
| Largura (bloco) | põe o bloco numa `minipage` da largura escolhida; 100% desfaz |
| Largura (imagem) | argumento opcional da imagem, `\imagem[0.5]{…}` |
| Fixar / Deixar flutuar | troca `\imagem` por `\imagemH` e vice-versa |
| Espaço antes / depois | `\vspace{0.5\baselineskip}` antes ou depois; negativo aperta |
| ↑ ↓ (ou Alt+↑ / Alt+↓) | sobe ou desce o bloco um lugar |
| ◂ pág. / pág. ▸ | leva o bloco para a página anterior ou seguinte |
| Nova página antes | `\newpage` antes do bloco |
| Com o próximo / com o anterior | os dois blocos lado a lado em colunas |
| Trocar lados, + Coluna, Iguais, Tirar da coluna, Desfazer colunas | mexem na linha de colunas |
| Duplicar, Código, Excluir | também Delete num bloco sem texto selecionado |

As camadas de letra e alinhamento são ambientes padrão do LaTeX, sem pacote
novo:

```latex
\begin{centering}
\begin{small}
Texto do bloco.

\end{small}

\end{centering}
```

### Arrastar

Arraste a alça **⠿** de um bloco:

- para antes ou depois de outro bloco, inclusive para dentro e para fora de
  caixas e colunas;
- para a borda direita ou esquerda de um bloco, e os dois ficam lado a lado;
- para o **espaço livre no fim de uma página**, que fica destacado. Se o
  bloco não couber ali, o LaTeX o leva para a página seguinte, e o editor
  mostra isso na compilação seguinte.

Perto da borda da janela a página rola sozinha, mais rápido quanto mais perto.
O divisor entre colunas e o canto azul de uma imagem também se arrastam.

### Espaço vertical

Uma linha vazia não existe no LaTeX, então o editor não finge que existe:
Enter numa linha vazia cria `\vspace{\baselineskip}`, e cada Enter seguinte
soma uma linha. O espaço aparece com a altura que terá no PDF. Para mudar,
arraste a borda de baixo dele ou use − / + na barra (¼ de linha). Os
`\vspace`, `\medskip` e `\bigskip` que já estão no arquivo aparecem do mesmo
jeito.

### Colunas e imagens

As colunas viram `minipage` lado a lado:

```latex
\par\noindent
\begin{minipage}[t]{0.48\linewidth}
\vspace{0pt}

...
\end{minipage}\hfill
\begin{minipage}[t]{0.48\linewidth}
\vspace{0pt}

...
\end{minipage}\par
```

O `\vspace{0pt}` alinha as colunas pelo topo. Dentro de uma coluna ou caixa,
um `\imagem` (figura flutuante) vira `\imagemH`, porque o LaTeX não aceita
figura flutuante ali.

Na primeira vez que você muda a largura de uma imagem, o editor pede para
alterar a definição do `\imagem` no preâmbulo: a largura passa a ser um
argumento opcional, e as imagens que já existem continuam com 92%. A
descrição usada para gerar a imagem só sai no PDF quando o arquivo da imagem
falta, então o editor a mostra recolhida, marcada como "não impresso".

O tectonic tem que estar instalado, em `~/.local/bin/tectonic` ou no caminho
da variável `TEX_EDITOR_TECTONIC`. O pdf.js e o KaTeX vêm de CDN, então o
editor precisa de internet.

## Arquivos que incluem outros (`\input`, `\include`)

Cada `.tex` é editado separadamente. Uma linha `\input{capitulos/cap03}`
aparece como um botão **Abrir**. **Salvar grava todos os arquivos
alterados**, porque uma mudança de diagramação pode mexer no preâmbulo
também. Os arquivos com alterações pendentes ficam marcados com ● na lista.

**Salvar como** oferece duas opções:

- **Cópia deste arquivo:** grava o arquivo como está, com os `\input`
  intactos. Eles continuam apontando para os mesmos arquivos. O editor passa
  a trabalhar na cópia, e o original fica como estava no disco.
- **Arquivo .tex completo:** troca cada `\input` e `\include` pelo conteúdo
  do arquivo, recursivamente, e grava um único `.tex` com marcas
  `% ===== início de … =====` e `% ===== fim de … =====`. Os caminhos são
  resolvidos como o LaTeX faz, a partir da pasta do arquivo principal. Um
  `\include` ganha os `\clearpage` que o LaTeX colocaria. As edições ainda
  não salvas entram como estão no editor. Use no arquivo principal, o que
  tem `\documentclass`, para obter um `.tex` que compila sozinho.

Limites do `.tex` completo:
- Imagens, `.sty` e `.bib` não são copiados. Salve na mesma pasta do original
  para os caminhos continuarem valendo.
- Um `\input` dentro de uma macro, como o `\incluipas` do preâmbulo, não é
  expandido, porque depende de condições avaliadas na compilação.
- Inclusões circulares e arquivos não encontrados são listados no aviso.

## Segurança

- Ao abrir, cada trecho é reconstruído a partir do modelo e comparado com o
  original. Um trecho que não bate fica "travado" e só pode ser editado como
  código. Nos 1.676 arquivos de `~/material/livros` nenhum trecho ficou
  travado.
- Na primeira gravação de cada arquivo, a versão do disco vai para
  `~/.local/share/tex-editor/backups/`.
- Se o arquivo mudar no disco (VS Code, git), o editor recarrega sozinho ou
  avisa quando há alterações não salvas. Ao salvar, ele recusa sobrescrever
  uma versão mais nova sem confirmação.

Para conferir a fidelidade numa pasta inteira:

```bash
tex-editor --check ~/material/livros
```

## Compilar

```bash
cargo build --release          # binário em target/release/tex-editor
cargo install --path .         # instala em ~/.cargo/bin
cargo test
```

## Como funciona

- `src/latex.rs` divide o arquivo em blocos que cobrem tudo, sem lacunas:
  parágrafo, item, título, ambiente, tabela, comentário, comando. Cada
  caractere exibido guarda o fonte que o gerou. Ao editar, o texto novo é
  alinhado com o antigo (prefixo, sufixo e LCS), e só o que mudou é
  reescapado.
- `src/main.rs` é um servidor HTTP local (`tiny_http`) com o estado de cada
  arquivo e o histórico de desfazer.
- `src/build.rs` achata o arquivo principal com as versões da tela, anotando
  a origem de cada linha. Depois compila com o tectonic numa thread e
  converte o SyncTeX em retângulos por linha do fonte.
- `src/ops.rs` tem as operações de diagramação sobre o fonte: mover, colunas,
  larguras, quebra de página e a largura opcional das macros de imagem.
- `web/` é a interface. `app.js` cuida da edição, `pages.js` do PDF, do modo página e das
  quebras de página, e `layout.js` da seleção, da barra de propriedades, da alça, do arraste, das colunas e das
  imagens. KaTeX, com mhchem, e pdf.js vêm de CDN.
