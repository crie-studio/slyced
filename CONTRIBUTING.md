# Contribuindo com o Slyced

Obrigado pelo interesse! O Slyced é mantido por uma pessoa, então relatos claros e mudanças
pequenas e bem testadas são as contribuições que mais ajudam.

## Antes de tudo: dados de pacientes

Este repositório é público. **Nunca envie exames DICOM, capturas de tela, logs ou mensagens com
dados de pacientes** (nome, ID, data de nascimento, número de acesso, hospital). Para mostrar um
problema, use o exame sintético do projeto (`cargo run --release --example phantom -- C:\temp\fantoma`)
ou anonimize os dados antes.

## Relatar problemas e sugerir ideias

- **Problema:** use o modelo [Problema](https://github.com/crie-studio/slyced/issues/new?template=problema.yml),
  com a versão, o pacote, o Windows e um trecho do `slyced.log` (sem dados de pacientes).
- **Sugestão:** use o modelo [Sugestão](https://github.com/crie-studio/slyced/issues/new?template=sugestao.yml)
  e conte o problema do dia a dia que ela resolve.
- **Falha de segurança:** não abra issue; siga a [política de segurança](SECURITY.md).

## Enviar código

1. Para mudanças maiores, abra um issue antes para combinar a ideia.
2. Prepare o ambiente como em [Compilação](README.md#compilação) (Rust e Visual Studio Build Tools).
3. Faça a mudança num branch e confira:

   ```powershell
   cargo test
   cargo clippy --all-targets
   ```

4. Teste no programa o que mudou. Os exemplos `phantom` (TC sintética) e `test_pacs` (mini-PACS
   local) permitem testar 2D, MPR, 3D, comparação e PACS sem exames reais.
5. Abra o pull request descrevendo o que muda e como foi testado.

### Convenções

- Textos da interface em **português do Brasil**, curtos e diretos; comentários no código em inglês.
- O executável deve continuar portátil: sem instalador, sem DLLs extras e sem dependência do
  Visual C++ Redistributable (o `build-release.ps1` confere).
- Nenhuma conexão de rede que o usuário não tenha pedido (veja a política de privacidade no README).
- Novidades visíveis entram no [CHANGELOG](CHANGELOG.md).

## Licença das contribuições

Ao enviar uma contribuição, você concorda em licenciá-la sob **MIT ou Apache 2.0**, as mesmas
licenças do projeto, sem termos adicionais.
