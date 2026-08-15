# Checklist de release LTS

Este documento define a validação mínima de uma versão estável do Beam antes
de sua publicação no Lyra OS. A publicação no OBS é deliberadamente manual: a
checklist existe para tornar esse processo repetível, verificável e seguro, não
para automatizar sua promoção.

Uma versão só deve ser promovida quando todos os itens aplicáveis estiverem
marcados. Exceções precisam ser registradas nas notas da versão, com impacto e
plano de correção.

## 1. Escopo e estado da versão

- [ ] O escopo está fechado e não há alterações não relacionadas incluídas.
- [ ] Não há bugs críticos ou altos conhecidos nos fluxos básicos de conexão.
- [ ] O diretório de trabalho está limpo e o commit candidato foi revisado.
- [ ] A versão coincide em `Cargo.toml`, `packaging/beam.spec` e AppStream.
- [ ] A data, descrição e versão da release estão atualizadas no AppStream.
- [ ] O `Cargo.lock` pertence ao commit candidato e não foi regenerado
      acidentalmente.
- [ ] Alterações de IronRDP, TLS, criptografia ou Secret Service tiveram seus
      changelogs e impactos de compatibilidade revisados.
- [ ] As limitações conhecidas e qualquer incompatibilidade estão documentadas.

## 2. Build e verificações estáticas

Executar em um ambiente limpo e compatível com a versão suportada do Lyra OS:

```sh
cargo build --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
bash tests/test-i18n.sh
```

- [ ] Todos os comandos acima terminaram com sucesso.
- [ ] O build release foi validado com `cargo build --release --locked`.
- [ ] Não há warnings novos aceitos sem justificativa.
- [ ] Os catálogos de tradução estão completos e sem mensagens fuzzy.
- [ ] O `.desktop` e os metadados AppStream passam nas validações do spec.
- [ ] Logs em nível normal não contêm senha, credenciais ou conteúdo do
      clipboard.

## 3. Compatibilidade e preservação de dados

Antes do teste de atualização, guardar uma cópia dos dados existentes do
usuário e registrar a versão de origem.

- [ ] Atualizar da última versão estável para a candidata preserva todos os
      perfis de conexão.
- [ ] `connections.toml` continua legível e nenhum campo é perdido.
- [ ] `known_hosts.toml` continua legível e mantém as decisões TOFU existentes.
- [ ] Credenciais existentes continuam acessíveis pelo Secret Service.
- [ ] Arquivos inválidos, parciais ou sem permissão produzem erro visível e não
      são substituídos silenciosamente.
- [ ] Criar, editar, duplicar e excluir perfis não afeta dados de outros perfis.
- [ ] Se o formato persistido mudou, a migração e sua recuperação foram
      testadas com dados reais da versão anterior.

## 4. Matriz manual de RDP

Registrar para cada servidor: sistema/versão, método de autenticação, resultado
e qualquer limitação observada. Testar, quando disponíveis para a versão:

- [ ] Windows 10.
- [ ] Windows 11.
- [ ] Windows Server suportado pelo Lyra OS.
- [ ] xrdp em uma distribuição Linux suportada.

Em cada servidor aplicável, validar:

- [ ] Primeira conexão e confirmação do certificado.
- [ ] Reconexão com certificado já confiável, sem novo prompt.
- [ ] Alteração do certificado gera aviso de divergência claro.
- [ ] Aceitar e recusar certificados funcionam como esperado.
- [ ] NLA/CredSSP com usuário local e, quando disponível, usuário de domínio.
- [ ] Senha correta, senha incorreta e correção de uma credencial salva inválida.
- [ ] Encerramento solicitado pelo usuário.
- [ ] Queda de rede, servidor encerrado e reconexão posterior.
- [ ] Cancelamento enquanto a conexão ainda está sendo estabelecida.
- [ ] Teclado comum, modificadores, AltGr, teclas estendidas e Ctrl+Alt+Del.
- [ ] Movimento, botões e roda do mouse.
- [ ] Clipboard de texto nos dois sentidos, incluindo Unicode e múltiplas linhas.
- [ ] Resoluções 16 e 32 bits, presets e resolução personalizada.
- [ ] Modo janela, tela cheia e atalho Ctrl+Alt+F12 para liberar a captura.
- [ ] Sessão contínua por pelo menos uma hora sem crescimento anormal de memória,
      travamento ou degradação progressiva de latência.

## 5. Cenários de robustez e segurança

- [ ] Host inexistente, porta fechada e servidor silencioso terminam com erro
      compreensível dentro do tempo esperado.
- [ ] Fechar a janela não deixa sessão ou tentativa de conexão ativa em segundo
      plano.
- [ ] Endereços DNS, IPv4 e IPv6 suportados conectam e usam uma chave TOFU
      consistente.
- [ ] Valores especiais em nome, host, usuário e domínio são exibidos
      literalmente, sem interpretação como markup.
- [ ] Uma UI lenta ou uma sessão com muitas atualizações não causa crescimento
      ilimitado das filas.
- [ ] Uso em 1080p e na maior resolução anunciada permanece responsivo.
- [ ] Falhas recuperáveis de entrada e clipboard ficam registradas sem revelar
      os dados transmitidos.
- [ ] Dependências afetadas por alertas de segurança conhecidos foram avaliadas.

## 6. Empacotamento e upload manual no OBS

- [ ] Gerar o arquivo de fontes a partir do commit/tag candidato, sem arquivos
      locais ou não rastreados.
- [ ] Gerar `vendor.tar.zst` a partir do mesmo `Cargo.lock` da versão candidata.
- [ ] Conferir que `.cargo/config.toml` aponta para `vendor` de forma relativa.
- [ ] Confirmar que o build vendorizado funciona sem acesso à rede.
- [ ] Conferir nomes, versões e checksums dos arquivos enviados ao OBS.
- [ ] Subir manualmente o tarball, `vendor.tar.zst` e o spec para o pacote de
      staging apropriado.
- [ ] Aguardar e revisar o resultado de todas as arquiteturas suportadas.
- [ ] Não ignorar warnings ou falhas de `%check`, validação AppStream ou
      instalação de arquivos.

## 7. Validação do pacote produzido

Os testes finais devem usar o RPM gerado pelo OBS, não o binário em `target/`.

- [ ] Instalar o RPM em uma instalação limpa e atualizada do Lyra OS.
- [ ] Confirmar que o aplicativo aparece no menu com nome, ícone e metadados
      corretos.
- [ ] Confirmar que todas as traduções empacotadas são carregadas.
- [ ] Repetir pelo menos o fluxo essencial: criar perfil, conectar, autenticar,
      usar entrada/clipboard, desconectar e reconectar.
- [ ] Atualizar uma instalação que contém a última versão estável e dados reais.
- [ ] Confirmar que a remoção do pacote não apaga dados do usuário.
- [ ] Verificar dependências, tamanho instalado e arquivos pertencentes ao RPM.

## 8. Rollback e promoção

- [ ] A versão estável anterior permanece disponível para rollback.
- [ ] O downgrade do pacote foi testado com os dados criados pela candidata.
- [ ] Quando o downgrade de dados não for seguro, isso está documentado antes da
      promoção e existe procedimento de restauração do backup.
- [ ] As notas da versão descrevem correções, limitações e mudanças relevantes
      para suporte.
- [ ] A tag aponta exatamente para o commit usado para gerar as fontes.
- [ ] O RPM testado é exatamente o artefato que será promovido.
- [ ] A versão foi promovida ao repositório estável somente após todas as etapas
      anteriores.

## Registro da validação

Copiar esta seção para as notas internas de cada release:

```text
Versão:
Commit/tag:
Data:
Responsável:
Ambiente Lyra OS:
Pacote/projeto OBS:
Servidores RDP testados:
Versão anterior usada no upgrade:
Rollback testado: sim/não
Exceções ou limitações aceitas:
Resultado final: aprovado/reprovado
```
