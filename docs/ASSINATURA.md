# Assinatura de código com a SignPath Foundation

A [SignPath Foundation](https://signpath.org/) assina de graça projetos de código aberto.
O certificado é da fundação: no Windows, o editor do `Slyced.exe` aparece como
**"SignPath Foundation"**, não com o nome do autor. Em troca, a fundação exige que os
executáveis sejam compilados a partir do código público, em servidores do GitHub, e que
cada assinatura seja aprovada por uma pessoa.

O repositório já está preparado:

| Arquivo | Para quê |
|---|---|
| `.github/workflows/release.yml` | Compila os 4 executáveis no GitHub Actions, envia ao SignPath, espera a aprovação, empacota os assinados e cria um rascunho de release |
| `.signpath/artifact-configuration.xml` | Diz ao SignPath quais arquivos assinar (os 4 `Slyced.exe`) e confere nome e versão do produto |
| `scripts/build-release.ps1` | `-NoZip` compila e confere; `-ZipOnly` empacota depois da assinatura |
| `LICENSE-MIT`, `LICENSE-APACHE` | Licenças de código aberto (exigência da fundação) |
| `README.md` | Seção "Política de assinatura de código", com a frase de atribuição, os papéis e a privacidade |

## Exigências da fundação

Resumo dos [termos](https://signpath.org/terms) (leia o original antes de se candidatar):

- Código aberto com licença aprovada pela OSI (o Slyced usa MIT ou Apache 2.0), sem
  licença comercial paralela, em repositório público.
- Projeto mantido e já lançado, com as funções descritas na página de download.
- Executáveis compilados pelo sistema de build (GitHub Actions em servidores do GitHub),
  nunca enviados de um computador pessoal.
- Autenticação de dois fatores (MFA) no GitHub e no SignPath para todos do projeto.
- Política de assinatura publicada na página do projeto (já está no README; se a página
  de download for o lfp.app.br, ela precisa mostrar a mesma seção ou um link para ela).
- Nada de malware nem de "programa potencialmente indesejado": o comportamento do
  programa precisa ser transparente para quem o usa.

## Passo a passo

### 1. Publicar o código no GitHub

1. Crie uma conta no GitHub (se ainda não tiver) e ative a autenticação de dois fatores.
2. Crie um repositório **público** vazio, por exemplo `slyced`.
3. Na pasta do projeto:

   ```powershell
   git init -b main
   git add .
   git commit -m "Slyced 1.5.4"
   git remote add origin https://github.com/<usuario>/slyced.git
   git push -u origin main
   ```

   O `.gitignore` já deixa de fora `target\`, `dist\` e os dados locais.

### 2. Candidatar-se

Preencha o formulário em <https://signpath.org/apply> com o endereço do repositório, a
descrição do Slyced e a página de download. A fundação analisa o projeto e, se aprovar,
cria a organização no SignPath e envia o convite.

### 3. Configurar o SignPath (depois da aprovação)

No [app.signpath.io](https://app.signpath.io/), com MFA ativado:

1. **Trusted Build Systems**: adicione o "GitHub.com" à organização e vincule ao projeto.
2. **Projeto** com o slug `slyced` (ou outro; veja o passo 4), ligado ao repositório.
3. **Artifact configuration** com o slug `slyced`: cole o conteúdo de
   `.signpath/artifact-configuration.xml`.
4. **Signing policy** de release (slug `release-signing`) com você como aprovador
   (*approver*), exigindo aprovação manual.
5. Crie um usuário de CI com permissão de enviar pedidos (*submitter*) nessa política e
   gere o **API token** dele.
6. Anote o **Organization ID** (aparece nas configurações da organização).

### 4. Configurar o GitHub

No repositório, **Settings → Secrets and variables → Actions**:

| Tipo | Nome | Valor |
|---|---|---|
| Secret | `SIGNPATH_API_TOKEN` | o API token do usuário de CI |
| Variable | `SIGNPATH_ORGANIZATION_ID` | o Organization ID |
| Variable (opcional) | `SIGNPATH_PROJECT_SLUG` | se o projeto não se chamar `slyced` |
| Variable (opcional) | `SIGNPATH_SIGNING_POLICY_SLUG` | se a política não se chamar `release-signing` |
| Variable (opcional) | `SIGNPATH_ARTIFACT_CONFIGURATION_SLUG` | se a configuração não se chamar `slyced` |
| Variable (opcional) | `SLYCED_NIGHTLY` | fixa o nightly das versões de Windows 7, ex.: `nightly-2026-11-05` |

### 5. Lançar uma versão

1. Atualize a versão (`Cargo.toml`, `assets/slyced.manifest` e CHANGELOG) e faça o
   commit.
2. Publique a tag com a mesma versão do `Cargo.toml`:

   ```powershell
   git tag v1.5.4
   git push origin v1.5.4
   ```

3. O GitHub Actions compila (cerca de meia hora) e envia o pedido ao SignPath. Você recebe
   um e-mail: confira o pedido no SignPath e **aprove**. O workflow espera até 4 horas.
4. Com a aprovação, o workflow confere as assinaturas, empacota os zips e cria um
   **rascunho** de release com os 4 pacotes e o `SHA256SUMS.txt`. Revise e publique.

Para testar sem criar release, rode o workflow em **Actions → Release → Run workflow**:
ele compila, pede a assinatura e deixa os pacotes só como artefatos do workflow.

Enquanto a variável `SIGNPATH_ORGANIZATION_ID` não existir, o workflow só compila, confere e
empacota: os pacotes ficam como artefato da execução, **sem assinatura**, e nenhum release é
criado (nem com tag).

## Conferir

```powershell
Get-AuthenticodeSignature .\Slyced.exe | Format-List Status, SignerCertificate
```

Deve mostrar `Valid` e `CN=SignPath Foundation`. Propriedades → Detalhes mostra
"Slyced DICOM Viewer" e a versão do release.

## Observações

- Compilações locais (`scripts\build-release.ps1`) **não** podem ser assinadas pela fundação.
- O código é público: qualquer pessoa pode ler, compilar e alterar o Slyced, dentro das
  licenças MIT/Apache.
- A reputação do certificado da fundação reduz os avisos do SmartScreen, mas cada versão
  nova ainda pode ser analisada pelos antivírus nos primeiros dias.
