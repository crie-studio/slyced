# Histórico de versões

Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/);
numeração em [versionamento semântico](https://semver.org/lang/pt-BR/).

## [Não lançado]

### Adicionado
- **Interface em inglês**, no programa para Windows e na versão web. O idioma segue o do Windows
  (ou do navegador): português nos sistemas em português, inglês nos demais. Dá para escolher em
  **Configurações → Aparência → Idioma** ou com "Português · English" na tela inicial, e a troca
  vale na hora, sem reiniciar. Em inglês, medidas e posições usam ponto decimal e as datas aparecem
  como "12 Mar 2024". Na versão web, `?lang=en` ou `?lang=pt` no endereço escolhe o idioma, e a
  escolha feita nas configurações fica guardada no navegador. A página do projeto em inglês mostra
  as telas com a interface em inglês.
- **Versão web (experimental)** em <https://crie-studio.github.io/slyced/app/>: o mesmo
  visualizador no navegador, com 2D, MPR e 3D. Abre pastas, arquivos e ZIP escolhidos ou
  arrastados para a página, sem enviá-los a nenhum servidor; `?exemplo` abre um exame
  sintético. Ainda sem PACS e sem JPEG-LS, com a carga em um único núcleo e sem guardar as
  configurações. Publicada pelo GitHub Pages a cada atualização da `main`.
- **Página do projeto** em <https://crie-studio.github.io/slyced/> (e em inglês, em `/en/`): apresentação com 3D ao
  vivo (fantoma sintético com corte anatômico), história "do corte ao volume", MPR interativo,
  recursos, privacidade, downloads e créditos. A janela **Sobre o Slyced** ganhou o link
  "Conheça o Slyced" para ela.
- **Site pronto para buscadores e IAs:** perguntas frequentes, dados estruturados, imagens de
  compartilhamento em português e inglês, `sitemap.xml`, `llms.txt`, página 404 e aviso ao Bing a
  cada publicação. Textos de divulgação e manifesto do winget em `docs/`.

### Alterado
- Em português, todos os números com casas decimais usam vírgula (espessura do slab, tempo da
  busca no PACS, registro na comparação, cálculos, calculadoras de hérnia e de volume), não só as
  medidas sobre a imagem. Os campos de diâmetro aceitam vírgula ou ponto.

### Corrigido
- O tour de boas-vindas fechava a versão web em telas mais estreitas que o cartão (celulares).

## [1.5.4] - 2026-11-09

### Removido
- **Prazo de uso**: o Slyced não expira mais. As versões 1.5.0 a 1.5.3 deixam de funcionar depois
  de 31/12/2026; esta e as próximas funcionam sem data limite.

### Adicionado
- **Preparação para a assinatura de código pela SignPath Foundation**: o GitHub Actions
  (`.github/workflows/release.yml`) compila os executáveis, envia para assinatura e empacota os
  assinados depois que cada pedido é aprovado; o Windows mostrará "SignPath Foundation" como
  editor. Ainda depende da aprovação da fundação (veja `docs/ASSINATURA.md`): os pacotes desta
  versão não são assinados.
- README com a política de assinatura de código e a declaração de privacidade.
- Os pacotes trazem o texto das licenças do Slyced (`LICENSE-MIT` e `LICENSE-APACHE`).
- Código aberto em <https://github.com/crie-studio/slyced>, com telas, guia de contribuição,
  política de segurança e modelos para relatar problemas e sugerir ideias.
- O `LEIA-ME.md` dos pacotes passa a ser o manual do usuário (`docs/MANUAL.md`): recursos,
  atalhos, PACS, portabilidade, solução de problemas, antivírus e privacidade.

### Alterado
- `build-release.ps1`: `-NoZip` só compila e confere os executáveis, `-ZipOnly` só empacota os que já
  estão em `dist\` (usado depois da assinatura); o `SHA256SUMS.txt` é recriado a cada empacotamento.

## [1.5.3] - 2026-11-06

### Corrigido
- **Busca em PACS que recusam a consulta** ("C-FIND falhou: falha no processamento (0xC000)"): quando o
  servidor recusa a busca, o Slyced repete sozinho de forma mais simples — sem os campos opcionais
  (número de séries/imagens, modalidades, médico solicitante), com intervalo de datas fechado e,
  por fim, só com os campos que todo PACS precisa aceitar e o curinga apenas no fim do nome
  ("SILVA*"). A forma que funcionou fica guardada para aquele servidor durante a sessão. Vale para
  a lista de estudos, as séries e a linha do tempo do paciente.
- A mensagem de erro do PACS mostra a explicação que o próprio servidor envia (Error Comment), e o
  slyced.log registra cada tentativa.
- ID do paciente e número de acesso não são mais convertidos para maiúsculas na busca.
- Se o PACS não aceitar C-GET, a mensagem diz como trocar para C-MOVE.

## [1.5.2] - 2026-11-05

### Alterado
- **Tela de abertura**: "Slyced" e o número da versão ficam à direita do logo, em vez de embaixo, e
  as duas colunas (logo, descrição e dica de arrastar; botões de abrir e recentes) ficam
  centralizadas na altura do card.

### Corrigido
- **Logo com os quatro cantos iguais**: o lado direito tinha metade do arredondamento do esquerdo
  (o degradê era montado em partes). Agora é uma forma só, com o mesmo raio e borda suave em todos
  os cantos; vale também para o monograma do autor, os botões em degradê e as barras de progresso.

## [1.5.1] - 2026-11-05

### Adicionado
- **Versão para Windows 7 SP1** (também 8 e 8.1), em 64 bits (`win7-x64`) e 32 bits
  (`win7-x86`), com os mesmos recursos e o mesmo tamanho das outras. Como o DirectX 12 não
  existe no Windows 7, o 3D usa Vulkan ou OpenGL 3.3; sem eles, o modo de compatibilidade
  (2D e MPR). Compilada com os alvos de Windows 7 do Rust e verificada para não depender de
  nenhuma função do Windows 8 ou mais novo.

### Alterado
- **3D por OpenGL**: o shader do volume compila em drivers OpenGL (um nome reservado do GLSL
  impedia) e o volume enviado à placa usa o tamanho máximo que o driver aceita, em vez de 256³.
- **Memória**: as séries abertas usam no máximo metade da memória do computador (PCs com 4 GB
  continuam respondendo); na versão de 32 bits, no máximo cerca de 1,4 GB.
- O aviso do modo de compatibilidade diz o que falta para o 3D (DirectX 12, Vulkan ou
  OpenGL 3.3), sem citar só o DirectX 12.

## [1.5.0] - 2026-11-04

### Alterado
- **Barra de título** sempre "Slyced - por Luiz Felipe Pitzer": o nome do paciente e da série não
  aparecem mais no título da janela nem na barra de tarefas do Windows.
- **Tela de abertura** em duas colunas: à esquerda o logo, a versão e o que o programa faz; à direita,
  Pasta, Arquivos e PACS com o mesmo destaque (antes "Abrir pasta" era o botão principal), cada um
  com uma linha dizendo o que abre e o atalho, e os exames recentes. Em janelas estreitas as colunas
  encolhem.

### Prazo de uso
- Esta versão funciona até 31/12/2026 (data e fuso deste computador). A partir de 01/01/2027 a
  janela continua mostrando o que estava aberto, apagado como marca d'água, com um card com o logo,
  o crédito e o aviso de que o programa expirou; cliques, teclas e arquivos arrastados deixam de ter
  efeito (a janela ainda pode ser fechada). O PACS não é mais consultado e o receptor não liga.

## [1.4.0] - 2026-11-04

### Adicionado
- **Prévia na barra de cortes**: ao passar o mouse na barra à direita da imagem aparece a imagem
  daquele corte (com a janela, a rotação e o espelhamento da tela), o número, a posição em mm e o
  nome da marca, se houver; o clique continua levando até ele. Funciona no 2D e no MPR.
- **Ir para (Ctrl+G)**: caixa sobre a janela ativa para ir a um corte pelo número ou a uma posição em
  mm; Alt+← volta para onde você estava.
- **Linha do tempo do paciente** no cartão da lista de exames: todos os exames do paciente por data,
  os deste computador (ponto cheio) e os do PACS (ponto vazado). Os da mesma região do corpo, ou da
  mesma modalidade quando a região não é conhecida, ficam destacados; ao parar o mouse aparecem
  data, descrição, modalidades e o motivo do destaque. Um clique compara com o exame aberto; se o
  exame estiver só no PACS, ele é baixado e a comparação abre ao terminar. O PACS só é consultado
  com o mesmo ID de paciente e data de nascimento (ou nome); dá para desligar em Configurações →
  Rede DICOM.
- **Tela cheia de leitura (F11)**: só as imagens, separadas por uma linha fina, sem moldura (a janela
  ativa ganha um canto azul) e com texto mínimo: plano, corte e janela. Ctrl+I troca entre texto
  completo, mínimo e nenhum (padrão em Configurações → Imagens). Ao mexer o mouse aparecem uma barra
  pequena (janela/nível, mover, zoom, medir, achado, predefinição de janela, layout, 2D/MPR/3D/A|B e
  "mais") e o botão de sair; parado, tudo some, inclusive o ponteiro. Encostar o mouse na borda
  esquerda (ou Tab) abre a lista de exames com a linha do tempo. Os avisos viram uma faixa pequena no topo.

### Alterado
- O **AE Title "LUMEN"**, que ficou de quando o programa tinha o nome antigo, passa a ser "SLYCED"
  (um aviso lembra de cadastrá-lo no PACS, se o servidor só aceita AEs conhecidos), e pacotes antigos
  do programa (`Lumen-….zip`) saem da lista de abertos recentemente.
- Ctrl+I também liga e desliga as informações sobre a imagem na tela normal.

## [1.3.1] - 2026-11-04

### Alterado
- **Lista de exames mais compacta**: paciente e exame no mesmo cartão, em duas linhas (cerca de 120
  para 50 px); ID, data relativa, séries, imagens e hospital aparecem ao parar o mouse. Exames
  anteriores do mesmo paciente ocupam uma linha. Cada série ocupa 40 px (antes 66), com miniatura,
  onde está aberta e número de imagens; modalidade e espessura ficam na dica.

### Corrigido
- **Localizador esbranquiçado**: a vista frontal agora é calculada como uma radiografia (atenuação
  somada, osso com mais peso) e a variação lenta da espessura do corpo é descontada, como no
  processamento de radiografias digitais; ossos, contornos e espaços aéreos aparecem em vez de uma
  silhueta branca.

## [1.3.0] - 2026-11-02

### Adicionado
- **Iluminação 3D selecionável** para VR e superfície, no painel do 3D e em Configurações → Imagens:
  - **Simples**: a luz de antes, vinda da câmera; a mais leve para a placa de vídeo.
  - **Realista** (padrão): luz lateral com sombras suaves, oclusão ambiente nas reentrâncias,
    contorno nas bordas e curva de tons de cinema, em tempo real.
  - **Cinemático**: com o volume parado, a imagem é refinada por alguns segundos (64 amostras com
    luz de área, oclusão por raios e anti-serrilhado); enquanto gira, usa a luz realista. Um aviso
    "Refinando a luz…" mostra o progresso.

## [1.2.0] - 2026-11-02

### Adicionado
- **Menu no botão direito** da imagem (clique sem arrastar; arrastar continua ajustando a janela):
  cursor 3D, achado ou densidade naquele ponto, medir distância/ROI/seta, abrir em MPR ou 3D, copiar
  ou salvar a imagem, restaurar e maximizar.
- **Trocar de série** com ← / → ou os botões laterais do mouse, e uma **tira de séries** com
  miniaturas embaixo das imagens (desligável em Configurações → Imagens).
- **Marcas na barra de cortes**: achados, medidas, focos de cálculo e cursor 3D; um clique na marca
  vai exatamente àquela imagem.
- **Localizador**: vista frontal do volume no canto das imagens axiais, com a linha do corte atual;
  clicar ou arrastar nela leva a outro nível (desligável em Configurações → Imagens).
- **Voltar e avançar** (Alt+← / Alt+→) para onde você estava antes de trocar de série, pular para
  um achado ou cálculo, ou de um salto pela barra de cortes ou pelo localizador.
- **Memória por série**: ao voltar a uma série, ela reabre no mesmo corte, janela, zoom e com as
  mesmas medidas.
- **Modo foco** (Tab, ou botão na barra) esconde os painéis e a tira de séries; **tela cheia** com
  F11 (Esc também sai). **Ctrl+C** copia a imagem da janela ativa.

## [1.1.3] - 2026-11-02

### Alterado
- As ferramentas de navegação (janela/nível, mover, zoom, cortes, mira MPR e cursor 3D) voltam a ficar
  à mostra na barra, uma por botão; Medir e Leitura continuam agrupadas.

## [1.1.2] - 2026-11-02

### Adicionado
- **Limpeza dos exames baixados do PACS**: as cópias baixadas há mais de 7, 30 ou 90 dias são
  apagadas ao abrir o Slyced (padrão: 30 dias; "Nunca" mantém tudo), com opção de apagar todas ao
  fechar o programa e um botão "Apagar agora" que mostra o espaço ocupado. Exames abertos no momento e
  exames abertos de pastas, CDs ou pendrives nunca são apagados.

### Alterado
- **Configurações redesenhadas**: janela maior com seções à esquerda (Aparência, Imagens e
  desempenho, Rede DICOM, Exames baixados, Dados e modo portátil, Sobre), cada ajuste com explicação
  e chaves liga/desliga. A seção Rede mostra se o receptor está ligado e liga/desliga na hora.
- Tela inicial: o nome Slyced e a versão ficam ao lado direito do logotipo.

## [1.1.1] - 2026-11-02

### Alterado
- **Novo ícone**: três fatias empilhadas, ligadas ao nome Slyced (antes era um anel). Vale para o
  executável, a janela, a barra de tarefas e o logo dentro do programa.
- Em janelas estreitas, as linhas do canto superior direito (hospital, descrição, data) que
  encostariam no nome do paciente deixam de ser mostradas.

## [1.1.0] - 2026-11-02

### Adicionado
- **Desfazer e refazer** (Ctrl+Z / Ctrl+Y e botões na barra): medidas, realces, filtros, janela, rotação,
  inversão, cursor 3D, focos de cálculo, trajeto do rastreador e planos oblíquos. Um arraste inteiro é
  um passo; abrir outras imagens começa um histórico novo.
- **Ferramentas por grupo**: Navegar, Medir e Leitura viram botões com menu (o botão usa a última
  ferramenta do grupo). A barra de status mostra como usar a ferramenta ativa e o ícone dela acompanha
  o cursor sobre a imagem.
- **Volume por 3 diâmetros** (menu Medir): largura, profundidade e altura pedidas em sequência, em
  qualquer janela ou plano; volume do elipsoide em mL e densidade média dentro dele.
- **Prévias**: os realces (gordura, gás livre, cálculos) aparecem aplicados ao corte atual antes de
  escolher, e cada predefinição 3D mostra uma miniatura renderizada do volume aberto.
- **Fila de downloads do PACS**: dois downloads por vez, os demais aguardam; "Tentar de novo" para
  falhas e cancelados, "Abrir" para os concluídos, e a lista também pela barra de status.
- **Aparência** nas Configurações: tema escuro, claro ou de alto contraste e tamanho da interface
  (80–150%). As imagens continuam sobre fundo escuro em todos os temas.
- **Tour de boas-vindas** na primeira vez (5 passos) e **novidades da versão** após atualizar; ambos
  podem ser abertos de novo pela ajuda (F1).
- "Fechar todos os exames" pede confirmação, e o aviso de exame fechado tem **Desfazer**.

## [1.0.1] - 2026-11-02

### Adicionado
- **Etiquetas de alterações sobre a imagem**: medidas, realces (gás, gordura, cálculos), slab, filtro
  de ruído, mapa de cores, rotação/espelhamento, inversão, cursor 3D, focos de cálculo, trajeto do
  rastreador e planos oblíquos aparecem como etiquetas no canto da janela ativa. O X de cada uma
  desfaz só aquilo; **Restaurar tudo** (Shift+R) volta a tela ao padrão. As etiquetas não entram na
  imagem exportada.
- **Botão Restaurar** na barra, com contador de alterações e menu: visualização da janela (R), apagar
  medidas e setas (Shift+Del), desligar realces e filtros, limpar cursor e marcadores, restaurar tudo.
  Os achados salvos e o alinhamento da comparação são mantidos.
- **Abrir arquivo** de volta à mão: com espaço, o botão **Abrir** da barra abre uma pasta e sua seta
  mostra arquivos/ZIP e os recentes; em janelas estreitas ficam os ícones de pasta e de arquivo. A
  lista de exames e a tela inicial também têm **Abrir arquivo**, e o menu ⋯ da lista mostra os recentes.

### Corrigido
- A barra superior mede o espaço disponível e troca os textos por ícones só quando não cabem (antes
  usava uma largura fixa).
- A calculadora de hérnia continua esperando a próxima medida quando medidas são apagadas no meio.

## [1.0.0] - 2026-11-01

Primeira versão estável, com o novo nome.

### Alterado
- **O Lumen agora se chama Slyced.** O executável é `Slyced.exe`, os dados ficam em
  `%LOCALAPPDATA%\Slyced` (ou `SlycedData` no modo portátil, ativado por `Slyced.portable`) e o
  log em `slyced.log`. Na primeira abertura, a pasta de dados do Lumen (configurações, servidores
  PACS, achados e exames baixados) é trazida automaticamente, inclusive no modo portátil. O AE Title
  já configurado é mantido; instalações novas usam `SLYCED`.
- A tela inicial mostra os exames recentes pelo nome da pasta (caminho completo ao passar o mouse).

### Verificado para a 1.0
- Testes automáticos de decodificação (JPEG 12 bits, sem perdas, JPEG-LS), volumes, MPR, CPR,
  registro entre exames, filtros, realces, cálculos, hérnia, fases e migração de dados.
- Cenários completos no programa: 2D e layouts, MPR com protocolos, MPR oblíquo e CPR, 3D,
  comparação (alinhamento automático, sobreposição, cortina, xadrez, fechar lados), fases e ΔHU,
  realces e busca de cálculos, calculadora de hérnia, rastreador tubular, fechar exames, ZIP,
  JPEG/JPEG-LS e PACS (C-FIND, C-GET, C-MOVE), sem erros no log.

## [0.6.1] - 2026-11-01

### Removido
- Integração com o TotalSegmentator (segmentação automática por IA), a pedido. O Lumen volta a não
  executar nenhum programa externo.

## [0.6.0] - 2026-11-01

### Adicionado
- **Fases sem × com contraste** (menu Protocolos): o Lumen identifica as fases pela descrição e
  pelo agente de contraste do DICOM e as abre lado a lado, alinhadas. A densidade pontual (H) lê o
  mesmo ponto nas duas fases (média numa esfera de 2,5 mm) e mostra
  "Pré 35 | Pós 38 | Δ +3 HU · sem realce" (< 10 HU sem realce, 10–20 duvidoso, ≥ 20 realce),
  também listado no painel. Em Exibição: **mapa de realce** (ganho de 15 a 80+ HU em amarelo →
  vermelho sobre a fase sem contraste) e **imagem de subtração**.
- **Segmentação automática com o TotalSegmentator** (instalado à parte): exporta a série em
  NIfTI, executa o TotalSegmentator neste computador (tarefas "total", "liver_segments" e outras;
  modo rápido; CPU/GPU), mostra o progresso e carrega as máscaras como camada colorida, com nome
  em português sob o mouse, lista com volume (mL), visibilidade, busca e ida até cada estrutura.
  Também carrega máscaras de uma pasta gerada em outro computador.

## [0.5.0] - 2026-11-01

### Adicionado
- Realces de leitura (botão ✦ na barra e painel "Realces"):
  - **Gordura mesentérica**: janela 250/−50 (também na tecla 0) e gordura densificada em laranja,
    ignorando a borda dos órgãos, para borramento inflamatório.
  - **Gás livre**: janela 1000/−600 em negativo e todo gás dentro do corpo (< −800 HU) em ciano;
    o ar em volta do paciente não é marcado.
  - **Cálculos**: tudo acima do limiar (130–800 HU) em magenta e busca no volume dos focos
    densos pequenos (cálculos, apendicolitos, calcificações), descartando osso e vasos
    contrastados; a lista leva até cada foco, marcado como C1, C2…
- Calculadora de hérnia incisional (menu Protocolos): volumes do saco herniário e da cavidade
  abdominal (elipsoide), índice de Tanaka, razão de Sabbagh, classe de largura EHS do defeito,
  medidas pela régua na imagem ou digitadas, e resumo copiável para o laudo.

### Alterado
- A reformatação curva virou o **rastreador tubular**: funciona também no visualizador 2D (cliques
  nos cortes axiais, a imagem esticada abre numa segunda janela), tem predefinições para
  apêndice, ureter, colédoco e vaso, e a roda do mouse sobre a imagem esticada a gira.
- Distâncias medidas para a calculadora de hérnia mostram o nome da medida.

## [0.4.0] - 2026-11-01

### Adicionado
- Densidade pontual: legenda com os valores típicos (HU) de cada tecido, destacando os que
  correspondem ao valor sob o cursor, e o tecido mais provável junto de cada ponto medido.
- Fechar: botão X no canto de cada janela (ou Ctrl+W) fecha a série; no MPR/3D volta ao 2D; na
  comparação fecha um dos lados (o restante vira o exame A). O X do cartão na lista fecha o exame
  inteiro e libera a memória; "Fechar todos os exames" no menu da lista.
- "Comparar com o exame aberto" no menu de cada exame da lista.

### Alterado
- Lista de exames redesenhada: agrupada por paciente, cartões com data, data relativa,
  "mais recente"/"anterior", número de séries e imagens; séries em lista com miniatura maior,
  espessura e onde estão abertas (ou em grade, pelo botão do cabeçalho).
- Tela do PACS redesenhada: filtros por nome, ID, acesso, descrição, período (inclusive 1 ano) e
  modalidade em botões; resultados ordenáveis, com dados do paciente, hora, acesso e número de
  séries; filtro local; painel do estudo selecionado com séries marcáveis para baixar só algumas;
  "Abrir" para estudos já baixados; mensagens de erro na própria tela; resultado do teste de
  conexão em cada servidor.
- O MPR abre com a ferramenta Mira selecionada.

## [0.3.0] - 2026-11-01

### Corrigido
- Erro "Could not decode pixel data: JPEG decoding failure" ao abrir exames em JPEG de 12 bits
  ("JPEG Extended", comum em TC/RM), JPEG aritmético e algumas variantes de JPEG sem perdas:
  um segundo decodificador é usado quando o primeiro recusa a imagem.
- A mensagem de erro de decodificação agora informa a sintaxe de transferência e a causa.

### Adicionado
- JPEG-LS (com e sem perdas).
- **Cursor 3D** (Ctrl+clique ou tecla L): leva todas as séries e planos do mesmo exame ao
  mesmo ponto anatômico, marcado em cada imagem.
- **Protocolos de leitura**: Abdome agudo (axial, coronal e sagital sincronizados, janela de
  abdome, média de 4 mm), Tórax — nódulos, Angio/TEP, Osso/coluna e Crânio/AVC, escolhendo a
  série mais adequada do exame.
- **Achados**: marcadores com nome (tecla B), listados no painel e guardados entre sessões;
  um clique reabre a série no mesmo ponto, janela e zoom.
- **Seta com texto** (tecla T) e **lupa** (tecla M).
- **Espessura (slab) no 2D**: MIP, MinIP ou média das imagens vizinhas.
- **Filtro de ruído** com preservação de bordas, só na exibição (as medidas usam os pixels originais).
- **MPR oblíquo**: arraste as alças da mira para inclinar os planos.
- **Reformatação curva (CPR)** (tecla V no MPR): clique ao longo de apêndice, ureter, vasos ou
  alças e veja a estrutura retificada, com rotação e largura ajustáveis.
- **Comparação de exames** (A|B): exame atual e anterior lado a lado, sobrepostos, em cortina
  ou em xadrez. Alinhamento exato no mesmo referencial; entre exames diferentes, pelo centro do
  corpo, por pontos de referência (1 ponto = translação, 3 ou mais = rotação) ou
  automático (informação mútua). Navegação, cursor, zoom e janela sincronizados (cada um pode ser
  desligado) e medidas pareadas com variação em %. Radiografias: alinhamento por 2 pontos
  (escala e rotação).

### Alterado
- Barra de ferramentas compacta em telas estreitas; girar/espelhar/inverter agrupados em um menu.

## [0.2.1] - 2026-11-01

### Corrigido
- O Lumen podia fechar sem nenhuma mensagem em computadores cuja placa de vídeo ou
  driver não aceita o renderizador 3D. Agora a GPU é testada antes de abrir a janela;
  sem DirectX 12/Vulkan utilizável, o Lumen usa renderização por software (WARP) ou o
  modo de compatibilidade OpenGL (2D e MPR, sem 3D).
- Erros da placa de vídeo durante o uso desativam só o 3D, em vez de fechar o programa.
- Qualquer falha agora mostra uma mensagem com o caminho do log.

### Adicionado
- `Lumen - diagnostico.cmd` / `Lumen.exe --diagnostico`: relatório do Windows e das placas de vídeo.
- `Lumen.exe --opengl` força o modo de compatibilidade.

## [0.2.0] - 2026-11-01

### Adicionado
- Abertura direta de arquivos `.zip` (e de pastas que contêm `.zip`), sem extrair nada para o disco.
- PACS via DICOMweb (QIDO-RS e WADO-RS), com autenticação Basic ou Bearer.
- Barra de rolagem de cortes na lateral das janelas 2D e MPR.
- Zoom sem roda do mouse: Ctrl + arrastar (2D e 3D), teclas `+`/`−` e botões no painel.
- Navegação sincronizada e linhas de referência entre janelas do mesmo estudo.
- Janela "Sobre o Lumen" e créditos do autor.

### Alterado
- Configurações, log e exames baixados agora ficam na pasta do usuário do Windows
  (`%LOCALAPPDATA%\Lumen`). O modo portátil (`LumenData\` ao lado do executável)
  passou a ser opcional, em Configurações → Armazenamento.
- Configurações salvas por versões anteriores em `LumenData\` são importadas automaticamente.
- Renderização 3D prefere DirectX 12 e tem menos artefatos de amostragem.

## [0.1.0] - 2026-10-31

### Adicionado
- Primeira versão: visualizador 2D, MPR (axial, coronal, sagital com slab), renderização
  3D na GPU, medições, tags DICOM, exportação PNG e PACS via C-ECHO, C-FIND, C-GET e C-MOVE.
