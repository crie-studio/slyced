# Política de segurança

## Versões com correções

Só a versão mais recente do Slyced recebe correções de segurança. Atualize pela
[página de releases](https://github.com/crie-studio/slyced/releases/latest).

## Como relatar uma falha

**Não abra um issue público.** Use o relato privado do GitHub:
[Security → Report a vulnerability](https://github.com/crie-studio/slyced/security/advisories/new).

Inclua a versão e o pacote do Slyced, o Windows, o que a falha permite e como reproduzi-la. Se
precisar de um arquivo DICOM para mostrar o problema, gere um sintético ou anonimizado: **nunca
envie dados de pacientes**.

Você recebe uma resposta assim que possível, normalmente em até 7 dias. Depois da correção, a
falha é descrita no CHANGELOG e, se você quiser, com o seu nome nos créditos.

## O que interessa

- Arquivos DICOM, DICOMDIR ou ZIP que travem, fechem o programa ou executem código ao serem abertos.
- Problemas no receptor DICOM (Storage SCP), nas conexões com o PACS ou no DICOMweb (HTTPS).
- Qualquer envio de dados pela rede que o usuário não tenha pedido.
- Executáveis publicados que não correspondam ao código deste repositório.
