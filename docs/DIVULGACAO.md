# Divulgação e buscadores

Tudo o que depende de uma conta sua, com os textos prontos para colar. O site já tem título e descrição para
busca, dados estruturados, perguntas frequentes, imagens de compartilhamento em português e inglês, versão em
inglês, `sitemap.xml`, `llms.txt`, página 404 e aviso automático ao Bing (IndexNow) a cada publicação.

Endereços:

- Página (português): <https://crie-studio.github.io/slyced/>
- Página (inglês): <https://crie-studio.github.io/slyced/en/>
- Versão web: <https://crie-studio.github.io/slyced/app/> (com um exame sintético: `…/app/?exemplo`)
- Download para Windows: <https://github.com/crie-studio/slyced/releases/latest>
- Código: <https://github.com/crie-studio/slyced>

Dica: publique um por dia, não todos juntos, e responda os comentários nas primeiras horas. Nunca use imagens
de pacientes: o exame sintético (`?exemplo`) serve para telas e vídeos.

## 1. Google Search Console

A tag de verificação já está na página inicial.

1. Em <https://search.google.com/search-console>, na propriedade `https://crie-studio.github.io/slyced/`
   (prefixo de URL), clique em **Verificar** (método "Tag HTML").
2. Em **Sitemaps**, envie `sitemap.xml`.
3. Em **Inspeção de URL**, peça a indexação de `https://crie-studio.github.io/slyced/` e de `…/en/`.

## 2. Bing Webmaster Tools (é o índice que alimenta o Copilot e parte do ChatGPT)

1. Em <https://www.bing.com/webmasters>, escolha **Importar do Google Search Console**: o site e o sitemap vêm
   juntos, sem nova verificação.
2. Se preferir verificar por arquivo, o Bing entrega um `BingSiteAuth.xml`: coloque-o em `web/landing/public/` e
   publique (tudo o que está nessa pasta vai para a raiz do site).

## 3. AlternativeTo (aparece muito em respostas do tipo "alternativas a X")

Em <https://alternativeto.net>, **Suggest new application**:

- **Name:** Slyced
- **Short description:** Free, portable DICOM viewer for Windows 7–11 and the browser: CT, MR, X-ray and US in
  2D, MPR and 3D, with study comparison and PACS. No installer, no telemetry.
- **Platforms:** Windows, Online/Web
- **License:** Free, Open Source (MIT / Apache 2.0)
- **Tags:** dicom, dicom-viewer, medical-imaging, pacs, portable, radiology
- **Alternative to:** RadiAnt DICOM Viewer, MicroDicom, Weasis, Horos, OHIF Viewer
- **Website:** <https://crie-studio.github.io/slyced/en/>
- **Observação:** a interface é em português.

## 4. Product Hunt

- **Name:** Slyced
- **Tagline (60):** A 20 MB DICOM viewer: CT and MRI in 3D, no install
- **Link:** <https://crie-studio.github.io/slyced/en/>
- **Description:** Slyced opens CT, MR, X-ray and ultrasound studies from a hospital CD, a folder, a ZIP or a
  PACS, with MPR, curved reformation and GPU volume rendering. One portable .exe for Windows 7 to 11, or the same
  code in the browser via WebAssembly. Free, open source, no telemetry, and nothing leaves your computer.
- **Primeiro comentário (maker):** I'm a doctor in Brazil and wanted a viewer that opens the CD a patient brings,
  on whatever PC the on-call room has, without installers or licenses. Slyced is written in Rust (dicom-rs, egui,
  wgpu). The UI is in English and Portuguese. Feedback very welcome, especially on DICOM files that don't open.

  *(Ajuste a primeira frase se não quiser se apresentar como médico.)*

## 5. Hacker News (Show HN)

- **Título:** Show HN: Slyced – a 20 MB portable DICOM viewer in Rust that also runs in the browser
- **URL:** <https://crie-studio.github.io/slyced/en/>
- **Texto (opcional):** Opens CT/MRI studies from a CD, folder, ZIP or PACS with MPR and GPU volume rendering.
  One .exe for Windows 7–11, and the same code compiled to WebAssembly (WebGPU/WebGL2). Nothing leaves your
  machine. UI in English and Portuguese.

## 6. Reddit

**r/rust**

- **Título:** I built a portable DICOM (CT/MRI) viewer in Rust: one 20 MB .exe from Windows 7 to 11, and the same
  code in the browser
- **Texto:**

  Slyced is a medical image viewer (CT, MR, X-ray) with MPR, curved reformation and GPU volume rendering. Stack:
  dicom-rs for parsing and codecs, egui/eframe for the UI, wgpu with WGSL ray casting, rayon for decoding.

  A few things that were fun to solve:
  - Windows 7 builds (tier 3 targets with -Z build-std) next to x64 and ARM64.
  - The same crate compiled to wasm32: no threads, so jobs run after each frame; files dropped in the page live
    in an in-memory filesystem; openjp2 needed a tiny malloc/calloc/realloc/free/strcpy shim.
  - A fallback ladder for broken GPU drivers: DX12/Vulkan, WARP, then OpenGL.

  Live demo with a synthetic phantom: <https://crie-studio.github.io/slyced/app/?exemplo&lang=en> · Code:
  <https://github.com/crie-studio/slyced> (MIT/Apache). The UI is in English and Portuguese; happy to answer questions.

**r/radiology** (leia as regras do subreddit antes; autopromoção pode exigir flair)

- **Título:** Free portable DICOM viewer for Windows 7–11 (and browser) — looking for feedback
- **Texto:** I made Slyced, a free, open-source viewer that opens CDs, folders, ZIPs and PACS (C-FIND/C-GET/C-MOVE,
  DICOMweb), with MPR, CPR, 3D and comparison with prior studies. No installer, no telemetry, nothing uploaded.
  It is not a certified medical device. The UI is in English and Portuguese. If you have a study that doesn't open or
  looks wrong, I'd love to know (without patient data, please). <https://crie-studio.github.io/slyced/en/>

## 7. Vídeo curto (Reels, Shorts, TikTok)

Um vídeo vertical de 20 a 30 segundos com o exame sintético. O gancho que mais funciona é uma dor conhecida
("o PC do plantão é de 2012", "ganhou um CD do hospital e não consegue abrir?"), seguida do programa abrindo o
exame na hora e do 3D. Termine com "link na bio" apontando para <https://crie-studio.github.io/slyced/>.

## 8. LinkedIn

Todo plantão tem aquele PC antigo e um paciente com o CD do exame na mão. Pensando nisso, criei o Slyced: um
visualizador DICOM gratuito e de código aberto que abre TC, RM, RX e US do CD, da pasta, do ZIP ou do PACS, com
MPR e 3D na placa de vídeo. É um único arquivo de 20 MB, sem instalação, do Windows 7 ao 11, e também funciona
direto no navegador. Nenhum exame sai do computador.

Conheça: <https://crie-studio.github.io/slyced/>

(Não é dispositivo médico certificado: não use como única base para diagnóstico.)

## 9. Artigo de bastidores (TabNews, Dev.to, Medium ou LinkedIn)

- **Título:** Fiz um visualizador de tomografia em Rust que roda do Windows 7 ao navegador
- **Roteiro:**
  1. O problema: o CD do exame e o PC do plantão (instaladores grandes, licença, Windows antigo).
  2. O que o Slyced faz: 2D, MPR, 3D, comparação, PACS; um .exe de 20 MB; nada sai do computador.
  3. Como foi feito: Rust, dicom-rs, egui, wgpu; o mesmo código em WebAssembly (sem threads, arquivos em
     memória, o "shim" de malloc do openjp2).
  4. O que vem por aí e como ajudar (issues, exames que não abrem, sem dados de pacientes).
  5. Links: página, app no navegador, código.

E, para listas do GitHub como "awesome-dicom" (pull request na seção de visualizadores):

```markdown
- [Slyced](https://github.com/crie-studio/slyced) - Portable DICOM viewer for Windows 7–11 and the browser (Rust, WebAssembly), with MPR, 3D and PACS. Free and open source.
```

## 10. winget (catálogo de programas do Windows)

O Slyced entra como programa portátil (o `Slyced.exe` dentro do zip), para Windows 10 e 11 em x64 e ARM64. A
forma mais simples é a ferramenta oficial:

```bash
winget install wingetcreate
```

```bash
wingetcreate new https://github.com/crie-studio/slyced/releases/download/v1.5.4/Slyced-1.5.4-x64.zip https://github.com/crie-studio/slyced/releases/download/v1.5.4/Slyced-1.5.4-arm64.zip
```

Ela pergunta os dados e abre o pedido (pull request) no repositório `microsoft/winget-pkgs` com a sua conta do
GitHub. Os arquivos já preenchidos para conferir estão em [`docs/winget/`](winget/); a cada versão nova, troque o
número e os SHA-256 (o GitHub mostra o de cada arquivo na página do release).
