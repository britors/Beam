# Beam

Cliente RDP (Remote Desktop Protocol) do ecossistema **Lyra Enterprise Linux**, para conexão com
máquinas Windows (desktops e servidores). Funciona em qualquer distribuição Linux moderna, com
integração visual e funcional prioritária com o Lyra (GNOME/Wayland).

- Protocolo RDP em Rust puro via [IronRDP](https://github.com/Devolutions/IronRDP) — sem FFI para
  libfreerdp.
- Interface em GTK4 + libadwaita.
- Credenciais protegidas no Serviço de Segredos do sistema (GNOME Keyring / KWallet) via
  [`oo7`](https://crates.io/crates/oo7) — nenhuma senha é gravada em disco em texto plano.
- Validação de certificado por confiança no primeiro uso (TOFU), como o `known_hosts` do SSH.

## Estrutura do repositório

- `beam-core`: motor de sessão RDP, sem dependência de nenhum toolkit gráfico.
- `beam-gtk`: frontend GTK4/libadwaita (binário `beam`).
- `data`: `.desktop`, metadados AppStream e ícones.

## Compilando

Dependências de sistema (nomes Fedora/openSUSE): `gtk4-devel`, `libadwaita-devel`, um compilador
Rust estável recente (`cargo`, `rustc`).

```sh
cargo build --release
./target/release/beam
```

Variável de ambiente `BEAM_LOG` controla o nível de log (`tracing-subscriber`), por exemplo
`BEAM_LOG=debug ./target/release/beam`. Senhas e conteúdo de clipboard nunca são registrados nos
logs.

## Uso

- **Ctrl+Alt+F12** libera a captura de teclado/mouse e sai da tela cheia — use se algum atalho
  local precisar funcionar durante uma sessão.
- Clicar de volta na área da sessão recaptura o teclado/mouse.
- Ctrl+Alt+Del pode ser enviado pelo botão no cabeçalho da janela de sessão.

## Limitações conhecidas (v1)

- Sem RemoteApp, RD Gateway, redirecionamento de drives/áudio/impressoras/smartcards/USB.
- Sem multi-monitor e sem redimensionamento dinâmico da resolução durante a sessão.
- O cursor remoto não é desenhado; a sessão sempre usa o cursor local do sistema.
- Clipboard: apenas texto (CF_UNICODETEXT), sem arquivos.
- Apenas RDP — sem VNC/SSH.

Essas limitações são decisões deliberadas de escopo para a v1, não bugs.

## Licença

GPL-3.0-or-later. Veja [`LICENSE`](LICENSE).
