# Slyced — manual do usuário

Visualizador de imagens médicas (TC, RM, RX, US…) para Windows. É um único `Slyced.exe`,
sem instalação e sem dependências externas.

Desenvolvido por **Luiz Felipe Pitzer** · <https://lfp.app.br> · página do projeto, downloads e
histórico de versões: <https://github.com/crie-studio/slyced>. Até a versão 0.6 o programa se
chamava Lumen; ao abrir o Slyced pela primeira vez, as configurações, os servidores PACS e os
achados do Lumen são trazidos automaticamente.

> **Aviso:** software sem certificação como dispositivo médico (ANVISA/FDA/CE).
> Não utilize como única base para diagnóstico.

- [Recursos](#recursos)
- [Uso e atalhos](#uso)
- [Configurando um PACS](#configurando-um-pacs)
- [Portabilidade e dados](#portabilidade)
- [Requisitos e solução de problemas](#requisitos-e-solução-de-problemas)
- [Antivírus e SmartScreen](#antivírus-e-smartscreen)
- [Privacidade](#privacidade)

## Recursos

- **Lista de exames** compacta: paciente e exame num cartão de duas linhas (ID, data relativa, número de séries e imagens e hospital ao parar o mouse), exames anteriores do mesmo paciente numa linha marcada "anterior", uma **linha do tempo do paciente** no cartão (todos os exames dele, neste computador e no PACS, por data; os da mesma região ou modalidade ficam destacados e um clique compara com o exame aberto, baixando do PACS se preciso) e uma linha por série com miniatura, onde está aberta e número de imagens. A grade de miniaturas grandes continua no botão da barra. Feche um exame pelo X do cartão ou uma janela pelo X do canto (Ctrl+W).
- **Abertura rápida**: pastas, arquivos, CDs/DVDs com DICOMDIR (varredura recursiva em paralelo, arquivos com ou sem extensão, com ou sem preâmbulo).
- **ZIP direto**: abra (ou arraste) um `.zip` de exames sem descompactar; pastas com `.zip` dentro também são lidas. Nada é extraído para o disco e o ZIP não fica travado. ZIPs protegidos por senha ainda não são suportados (o Slyced avisa).
- **Sintaxes de transferência**: implícita/explícita, JPEG baseline, estendido de 12 bits, aritmético e sem perdas, JPEG-LS, JPEG 2000, RLE, deflate; multiframe e imagens coloridas.
- **2D**: janela/nível com predefinições de TC (teclas 1–9), zoom, mover, rotação, espelhamento, inversão, mapas de cor, cine e layouts 1×1 a 3×2. Arraste séries do painel para qualquer janela.
- **Navegação ligada**: janelas do mesmo estudo sincronizadas por posição anatômica (inclusive entre planos diferentes), linhas de referência do corte ativo (ex.: localizador) e **cursor 3D** (Ctrl+clique) que leva todas as séries e planos ao mesmo ponto.
- **Ferramentas na barra**: as de navegação (janela/nível, mover, zoom, cortes, mira, cursor 3D) ficam à mostra; **Medir** e **Leitura** agrupam as demais, e o botão usa a última ferramenta do grupo enquanto a seta mostra todas. A barra de status explica como usar a ferramenta ativa, e o ícone dela acompanha o cursor.
- **Medições e anotações**: distância, ângulo, ROI elíptica (média, DP, mín., máx., área), densidade pontual (HU, com o tecido mais provável e uma legenda dos valores típicos de cada tecido) e seta com texto.
- **Volume por 3 diâmetros** (menu Medir): desenhe largura, profundidade e altura (pedidas uma depois da outra, em qualquer plano) e veja o volume do elipsoide em mL e a densidade média dentro dele.
- **Navegação rápida**: ← / → (ou os botões laterais do mouse) trocam de série e uma tira de miniaturas embaixo das imagens mostra todas as séries do exame; a barra de cortes tem marcas de achados, medidas, focos de cálculo e do cursor 3D e, ao passar o mouse, mostra a imagem daquele corte antes de clicar; **Ctrl+G** vai a um corte pelo número ou pela posição em mm; um **localizador** (vista frontal do volume, processada como radiografia, com ossos e contornos realçados) no canto das imagens axiais leva a qualquer nível; **Alt+← / Alt+→** voltam e avançam para onde você estava; cada série reabre onde você parou.
- **Menu no botão direito** (clique sem arrastar): cursor 3D, achado ou densidade naquele ponto, medir, abrir em MPR/3D, copiar (Ctrl+C) ou salvar a imagem, restaurar e maximizar.
- **Tela cheia de leitura** (F11): só as imagens, com texto mínimo (plano, corte e janela; Ctrl+I troca entre completo, mínimo e nenhum). Ao mexer o mouse aparece uma barra pequena com as ferramentas principais, a janela, o layout e o espaço de trabalho, que some quando o mouse para; encostar o mouse à esquerda (ou Tab) abre a lista de exames. **Modo foco** (Tab) esconde só os painéis laterais.
- **Desfazer e refazer** (`Ctrl+Z` / `Ctrl+Y`): medidas, realces, filtros, janela, rotação, cursor 3D, trajeto do rastreador e planos oblíquos.
- **Achados**: marque pontos com nome (tecla B); a lista fica no painel, é guardada entre sessões e reabre a série no mesmo ponto, janela e zoom.
- **Restaurar**: o que as ferramentas mudaram aparece em etiquetas no canto da imagem (ex.: "3 medidas", "Gás livre", "Slab 6 mm", "Cursor 3D"); o X de cada uma desfaz só aquilo e **Restaurar tudo** volta tudo ao padrão (Shift+R). O botão Restaurar da barra mostra quantas alterações há e tem as mesmas opções em menu: visualização da janela (R), medidas e setas (Shift+Del), realces e filtros, cursor e marcadores. Os achados salvos são mantidos.
- **Leitura**: espessura (slab) também no 2D, filtro de ruído que preserva bordas (só na exibição) e lupa.
- **Realces de leitura (TC)**, com prévia de cada um no corte atual: **Gordura mesentérica** (janela 250/−50 com a gordura densificada em laranja, para borramento inflamatório), **Gás livre** (janela 1000/−600 em negativo com todo gás dentro do corpo, abaixo de −800 HU, em ciano: pneumoperitônio e retropneumoperitônio) e **Cálculos** (tudo acima do limiar em magenta; a busca no volume lista os focos densos pequenos, descartando osso e vasos, e leva até cada um).
- **Fases sem × com contraste** (menu Protocolos): abre as duas fases lado a lado e alinhadas; a densidade pontual (H) lê o mesmo ponto nas duas (média em 2,5 mm) e mostra o ΔHU com a leitura (sem realce, duvidoso, realce presente). Há também **mapa de realce** (ganho ≥ 15 HU colorido sobre a fase sem contraste) e **imagem de subtração**.
- **Calculadora de hérnia incisional** (menu Protocolos): volumes do saco e da cavidade pelo elipsoide, índice de Tanaka, razão de Sabbagh e largura do defeito (EHS), com medidas desenhadas na imagem ou digitadas e resumo para o laudo.
- **Protocolos de leitura**: Abdome agudo, Tórax — nódulos, Angio/TEP, Osso/coluna e Crânio/AVC montam o MPR com a série, a janela e o slab adequados em um clique.
- **MPR**: axial, coronal e sagital reconstruídos do volume, linhas de referência sincronizadas, espessura (slab) com MIP, MinIP ou média, **planos oblíquos** (arraste as alças da mira) e **rastreador tubular (CPR)**: 3 ou 4 cliques ao longo do apêndice, ureter, colédoco ou vaso, no 2D ou no MPR, geram a estrutura inteira esticada em uma imagem (a roda do mouse a gira).
- **Comparação de exames** (A|B): exame atual e anterior lado a lado, sobrepostos (transparência), em cortina ou em xadrez, com navegação, cursor, zoom e janela sincronizados. O alinhamento é exato quando os exames compartilham o referencial; entre exames diferentes é feito pelo centro do corpo, por pontos de referência ou automaticamente (informação mútua, funciona entre fases e modalidades). Medidas pareadas mostram a variação em %. Radiografias alinham por 2 pontos (escala e rotação).
- **3D na GPU**: ray casting (DirectX 12, Vulkan ou OpenGL 3.3) com VR por função de transferência (osso, angio, músculo, pele, pulmão, partes moles, RM — cada predefinição com uma prévia do próprio volume), MIP, MinIP e superfície; recorte por eixo, orientações rápidas e qualidade reduzida automaticamente durante a interação.
- **Iluminação 3D em três níveis** (painel do 3D ou Configurações → Imagens): **Simples** (luz da câmera, a mais leve), **Realista** (luz lateral com sombras suaves, oclusão nas reentrâncias, contorno e curva de tons de cinema, em tempo real) e **Cinemático** (com o volume parado, a imagem é refinada por alguns segundos, somando dezenas de amostras de luz, sombra e anti-serrilhado; girando, usa a luz realista).
- **PACS** (o mesmo tipo de servidor que o RadiAnt acessa): C-ECHO, C-FIND (estudo/série), recuperação por **C-GET** (sem porta de entrada) ou **C-MOVE** (com receptor Storage SCP embutido). Também **DICOMweb** (HTTP/HTTPS): pesquisa por QIDO-RS e download por WADO-RS, com usuário/senha ou token. Os downloads formam uma fila (dois por vez), com "Tentar de novo" e "Abrir", acessível também pela barra de status.
- **Tags DICOM** com filtro, exportação PNG da janela ativa, overlays com dados do paciente, orientação anatômica e régua.
- **Tela de abertura** em duas colunas: o programa à esquerda; à direita, Pasta, Arquivos e PACS com o mesmo destaque, cada um com uma linha dizendo o que abre e o atalho, e os exames recentes. A barra de título mostra só "Slyced - por Luiz Felipe Pitzer", nunca o nome do paciente.
- **Aparência** (Configurações): tema escuro, claro ou de alto contraste e tamanho da interface (80–150%). Um tour de boas-vindas mostra os controles principais na primeira vez (e pode ser refeito pela ajuda, F1).
- **Fechar com segurança**: "Fechar todos os exames" pede confirmação, e o aviso de exame fechado tem **Desfazer**.

## Uso

| Ação | Como |
|---|---|
| Abrir | botão **Abrir** da barra (clique: pasta; seta: arquivos, ZIP e recentes) — em janelas estreitas, ícones de pasta e de arquivo; `Ctrl+O` (pasta), `Ctrl+Shift+O` (arquivos) ou arrastar para a janela. Os recentes também ficam no menu ⋯ da lista de exames |
| Navegar cortes | roda do mouse, barra à direita da imagem (com marcas e prévia do corte), localizador no canto, ↑ ↓, PgUp/PgDn, Home/End, `Ctrl+G` (corte ou mm) |
| Trocar de série | ← / →, botões laterais do mouse ou a tira de séries embaixo das imagens |
| Voltar / avançar | `Alt+←` / `Alt+→`: para onde você estava antes de um salto |
| Menu da imagem | clique com o botão direito (arrastar com ele ajusta a janela) |
| Foco / tela cheia de leitura | `Tab` / `F11` · `Ctrl+I` texto sobre a imagem · `Ctrl+C` copia a imagem |
| Janela/nível | botão direito arrastando (ou ferramenta W) |
| Zoom | `Ctrl`+roda, `Ctrl`+arrastar (esquerdo), teclas `+`/`−` ou botões no painel |
| Mover | botão do meio ou `Shift`+esquerdo |
| Maximizar janela | duplo clique |
| Ferramentas | botões de navegação e grupos Medir e Leitura da barra, ou as teclas W, P, Z, S, C (mira MPR), D, A, E, H, T (seta) — `Del` remove a última medição, `Shift+Del` apaga todas |
| Desfazer / refazer | `Ctrl+Z` / `Ctrl+Y` (ou os botões da barra) |
| Restaurar | X nas etiquetas do canto da imagem desfaz cada alteração; `R` restaura a visualização da janela; `Shift+R` (ou **Restaurar tudo**) limpa medidas, realces, filtros, cursor e planos oblíquos |
| Cursor 3D | `Ctrl`+clique (ou ferramenta L); `Esc` apaga |
| Achado / lupa | B / M |
| MPR oblíquo | com a mira (C), arraste as alças coloridas das linhas |
| Rastreador tubular (CPR) | V no 2D ou MPR: clique ao longo da estrutura (role os cortes entre cliques); botão direito ou `Enter` conclui; roda gira a imagem esticada |
| Realces | menu **Leitura** da barra, com prévias: gordura mesentérica, gás livre, cálculos (tecla 0 = janela de gordura) |
| Comparar | botão **A\|B**; o exame anterior do mesmo paciente é escolhido sozinho (ou arraste uma série para a janela B); G marca pontos de referência |
| 3D | esquerdo gira, direito ajusta a função de transferência, meio (ou `Shift`+esquerdo) move, roda ou `Ctrl`+arrastar aproxima |

Tudo funciona também com mouse sem roda ou sem botão do meio.

`F1` mostra todos os atalhos.

## Configurando um PACS

1. Clique em **PACS → Adicionar servidor** e informe nome, AE Title, IP e porta.
   Na pesquisa, filtre por nome, ID, acesso, descrição, período e modalidade; ordene os
   resultados clicando nos títulos das colunas, veja as séries do estudo selecionado e baixe o
   estudo inteiro ou só as séries marcadas. Estudos que já estão no computador mostram **Abrir**.
2. Escolha **C-GET** (recomendado: não abre porta no Windows) ou **C-MOVE**.
3. Para C-MOVE, cadastre no PACS o AE local (padrão `SLYCED`) com o IP deste
   computador e a porta do receptor (padrão `11112`, em ⚙ Configurações). O Windows
   pode pedir liberação do firewall na primeira vez.
4. Use o ícone de tomada para testar (C-ECHO), pesquise e clique em ⬇ para baixar e abrir.

Se o PACS recusar a busca ("falha no processamento (0xC000)"), o Slyced tenta de novo sozinho
com uma consulta mais simples (só os campos que todo PACS precisa aceitar). Se ainda assim falhar,
a mensagem traz a explicação do servidor; filtre por nome, ID ou data e, se continuar, envie o
`slyced.log` (em `%LOCALAPPDATA%\Slyced`) a quem cuida do PACS. Se o download der erro de C-GET,
troque para C-MOVE na edição do servidor.

Para **DICOMweb**, escolha o protocolo DICOMweb e informe a URL base (no Orthanc,
`http://servidor:8042/dicom-web`). Não é preciso abrir porta neste computador. HTTPS
usa os certificados confiáveis do Windows (inclusive a CA interna do hospital, se
instalada). Senha ou token ficam gravados em texto no `config.json`.

## Portabilidade

O Slyced não precisa de instalação: basta executar o `Slyced.exe`. O runtime C é
ligado estaticamente, então não é necessário o Visual C++ Redistributable.

Configurações, servidores PACS, log e exames baixados ficam na pasta do usuário
do Windows, `%LOCALAPPDATA%\Slyced` (ex.: `C:\Users\maria\AppData\Local\Slyced`):
cada pessoa que usa o computador tem os seus. Nada é gravado ao lado do executável.

Os exames baixados do PACS ficam em `%LOCALAPPDATA%\Slyced\PACS\` (uma pasta por estudo) e são
apagados sozinhos depois de 30 dias. Em ⚙ Configurações → Exames baixados dá para escolher 7, 30 ou
90 dias ou "Nunca", apagar todos ao fechar o Slyced (computadores compartilhados) ou apagar agora.
Exames abertos de pastas, CDs ou pendrives nunca são apagados.

**Modo portátil** (pendrive): em ⚙ Configurações → Armazenamento, marque "Salvar
dados junto ao executável" (ou crie um arquivo vazio `Slyced.portable` ao lado do
`.exe`). A partir da próxima abertura, os dados ficam em `SlycedData\` ao lado do
executável. Configurações de versões anteriores que ficavam em `SlycedData\` são
importadas automaticamente para a pasta do usuário.

## Requisitos e solução de problemas

- **Windows 10 ou 11.** Use o pacote **x64** em PCs Intel/AMD (quase todos) e o
  **ARM64** só em PCs com processador Snapdragon.
- **Windows 7 SP1** (e 8/8.1): use o pacote **win7-x64** no Windows de 64 bits ou o
  **win7-x86** no de 32 bits (Painel de Controle → Sistema mostra qual é). Tudo funciona
  como nas outras versões; o 3D usa OpenGL 3.3 ou Vulkan, porque o DirectX 12 não existe
  no Windows 7, e com placas antigas sem esses recursos o Slyced abre no modo de
  compatibilidade (2D e MPR). A versão de 32 bits usa no máximo cerca de 1,4 GB para as
  séries abertas; exames muito grandes saem e voltam da memória mais vezes. PACS por
  DICOMweb com HTTPS precisa do TLS 1.2, que chega ao Windows 7 pelas atualizações do
  Windows Update.
- **Vídeo:** com DirectX 12, Vulkan ou OpenGL 3.3 tudo funciona, inclusive o 3D. Sem placa de
  vídeo adequada (máquinas virtuais, acesso remoto), o Slyced usa a renderização por
  software do Windows (WARP), mais lenta. Se nem isso funcionar, abre em
  **modo de compatibilidade OpenGL**, com 2D e MPR e sem 3D.
- **Não abriu?** Execute `Slyced - diagnostico.cmd` (junto do `Slyced.exe`): ele mostra
  um relatório do Windows e das placas de vídeo e salva `diagnostico.txt` na pasta de
  dados. O log fica em `%LOCALAPPDATA%\Slyced\slyced.log`. Para forçar o modo OpenGL:
  `Slyced.exe --opengl`.
- Se o Slyced fecha sem nenhuma mensagem, provavelmente foi bloqueado pelo antivírus
  ou pela política de TI do hospital: veja a seção abaixo.
- **Memória:** as séries abertas usam no máximo o valor de Configurações → Imagens e,
  em qualquer caso, metade da memória do computador; as usadas há mais tempo saem da
  memória e voltam do disco quando precisam.
- **Problemas e sugestões:** abra um chamado em <https://github.com/crie-studio/slyced/issues>
  (nunca anexe exames ou dados de pacientes).

## Antivírus e SmartScreen

O executável foi preparado para não parecer suspeito para heurísticas de antivírus:

- sem compactadores (UPX etc.), sem ofuscação e sem downloads ou execução de código externo;
- recursos de versão, ícone e manifesto completos (`asInvoker`: nunca pede administrador);
- nenhuma porta é aberta a menos que o receptor DICOM seja ligado pelo usuário;
- compilado com o toolchain oficial MSVC.

Os releases assinados usam o certificado da **SignPath Foundation** (veja a
[política de assinatura de código](https://github.com/crie-studio/slyced#política-de-assinatura-de-código-code-signing-policy)):
em Propriedades → Assinaturas Digitais do `Slyced.exe`, o editor aparece como
"SignPath Foundation". Se a aba Assinaturas Digitais não existir, o executável não está
assinado (como os compilados por conta própria): executáveis não assinados mostram o aviso
do SmartScreen ("O Windows protegeu o computador": clique em **Mais informações →
Executar assim mesmo**) até ganharem reputação, e alguns antivírus corporativos
bloqueiam programas desconhecidos por política.

1. Para conferir a assinatura no PowerShell: `Get-AuthenticodeSignature .\Slyced.exe`
   deve mostrar `Valid` e `CN=SignPath Foundation`.
2. Para conferir se o arquivo é o original: `Get-FileHash .\Slyced.exe` deve dar o mesmo
   SHA-256 do `SHA256SUMS.txt` publicado com o release.
3. Em caso de falso positivo, envie o arquivo para análise em
   <https://www.microsoft.com/wdsi/filesubmission> (e ao fabricante do antivírus usado).
4. Em ambientes gerenciados, peça ao TI para liberar o editor "SignPath Foundation" ou o
   hash SHA-256.

## Privacidade

Este programa não transfere nenhuma informação para outros sistemas em rede, a não ser
quando solicitado pelo usuário ou por quem o instala ou opera. Na prática: o Slyced só se
conecta aos servidores PACS (DICOM ou DICOMweb) que o próprio usuário cadastra, e só
quando ele busca ou baixa exames; a porta do receptor DICOM só é aberta se o
usuário ligar o receptor; o site do autor e a página do projeto só são abertos no navegador
quando o usuário clica nos links. Não há telemetria, estatísticas de uso nem verificação automática de atualizações.

## Licença

O Slyced é distribuído sob a licença **MIT** ou a **Apache 2.0**, à escolha de quem o usa
(`LICENSE-MIT` e `LICENSE-APACHE`, junto deste arquivo). Os ícones Phosphor têm licença
própria (MIT, `LICENSE-Phosphor.txt`).
