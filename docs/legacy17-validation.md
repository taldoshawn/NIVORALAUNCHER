# Validação do transporte nativo 1.7

O teste `tools/test_legacy17_server.py` conecta um probe Rust a um servidor
vanilla descartável, restrito a `127.0.0.1`. O servidor Java é uma referência de
desenvolvimento. Ele não integra o launcher, o APK nem um backend de jogo.

## Executar

São necessários o Rust definido em `rust-toolchain.toml`, Python 3 e Java.
O OpenJDK 21 disponível no ambiente executou ambas as referências.

```sh
python3 tools/test_legacy17_server.py --version 1.7.10
python3 tools/test_legacy17_server.py --version 1.7.2
```

O harness compila somente o exemplo `legacy17_server_probe` do crate
`horizon-protocol`, com dois workers. Por padrão, coloca a compilação em
`.local/parity-target`; respeita `CARGO_TARGET_DIR` quando definido. Para usar
um executável já compilado:

```sh
python3 tools/test_legacy17_server.py --version 1.7.10 --skip-build \
  --probe .local/parity-target/debug/examples/legacy17_server_probe
```

O probe aceita somente endereços de loopback e as versões 1.7.2 e 1.7.10. A
autenticação offline existe apenas no servidor temporário deste teste. A
configuração de servidores e contas do jogador não é alterada.

## O que o teste verifica

- Login e entrada em Play usando o protocolo selecionado.
- Respostas de keepalive aceitas pelo servidor durante a sessão.
- Descompressão de colunas e adaptação para o formato interno do renderer,
  com valores independentes para pedra, ar e lã vermelha com metadado 14.
- Inventário recebido do servidor e utilizado pelo adaptador das ações.
- Teleporte absoluto, sua confirmação e o campo separado de altura dos olhos
  (*stance*) exigido pelo movimento da 1.7.
- Colocação e quebra de um bloco por ações do cliente, confirmadas pelas
  atualizações de bloco enviadas pelo servidor.
- Movimento até `[1.5, 64.0, 0.5]`, confirmado separadamente pela leitura do
  arquivo de jogador que o servidor vanilla salvou.

Os caminhos exercitados são `legacy17::login`, `legacy17::bridge::Incoming`,
o modelo existente `legacy18::bridge::Outbound` e
`legacy17::bridge::outgoing`. O formato 47 é uma representação interna entre
adaptadores; as ações são recodificadas para 4 ou 5 antes de chegar ao socket.

## Evidência obtida

Nos testes reais realizados em 27 de novembro de 2026, ambas as referências
passaram login, teleporte, inventário, colocação, quebra, dois keepalives e
confirmação do movimento no save do servidor.

| Referência | Protocolo | Colunas recebidas | Estados decodificados | Posição salva pelo servidor |
|---|---:|---:|---:|---|
| 1.7.2 | 4 | 81 | 1.331.200 | `[1.5, 64.0, 0.5]` |
| 1.7.10 | 5 | 81 | 1.331.200 | `[1.5, 64.0, 0.5]` |

O total de estados decodificados não é uma contagem de regras de jogo validadas.

| JAR oficial de referência | SHA-1 |
|---|---|
| 1.7.2 | `3716cac82982e7c2eb09f83028b555e9ea606002` |
| 1.7.10 | `952438ac4e01b4d115c5fc38f891710c4941df29` |

O downloader existente verifica a metadata da versão; o harness confere tamanho
e SHA-1 do JAR, inclusive quando ele já está no cache. Os arquivos originais
ficam somente em `.local/datafix/servers/` e não são publicados no repositório.

Cada execução grava `result.json`, `server.log`, `probe-stdout.txt` e
`probe-stderr.txt` em `.local/legacy17-evidence/<versão>/`, ou na pasta indicada
por `--evidence`. O mundo e o jogador são temporários e removidos ao terminar.

## Verificações do código

| Verificação executada | Resultado |
|---|---|
| Biblioteca de protocolo e integração 1.8 existente | 188 + 12 testes passaram |
| Novos testes `legacy17`, `legacy17_chunks`, `legacy17_bridge_review`, `legacy17_limits` | 10 + 10 + 7 + 5 testes passaram |
| `horizon-world`, filtro `nbt::tests` | 15 testes passaram, incluindo os três novos casos históricos |
| Binário do cliente, teste `legacy17_selection_cannot_start_a_different_versions_local_engine` | Passou: recusa mundos locais, quick play, benchmark e dedicado com 1.7; aceita servidor 1.7 e troca explícita de versão |
| `cargo check -p horizon-client --bin horizon-client --no-default-features --locked` | Passou |
| `cargo check -p horizon-android --target aarch64-linux-android --no-default-features --locked -j2` | Passou, incluindo a integração final da lista de servidores |
| Formatação dos novos arquivos Rust, compilação do harness Python, whitespace do diff e `check_public_tree.py` | Passaram |

O check Android usou Rust 1.95.0 e NDK r28c (28.2.13676358), com compiladores
para API 28. Não gerou nem publicou APK. Os checks mantiveram os avisos
preexistentes e não habilitaram os recursos opcionais de áudio e controles.

O teste do binário exigiu corrigir dois erros anteriores restritos aos testes:
remover uma cópia duplicada de `not_built` para os testes usarem a implementação
real e completar o campo `has_settings` de uma fixture de shader. Nenhum teste
foi removido.

### Verificações que continuam bloqueadas

O teste novo de classificação visual de status
`multiplayer::tests::historical_status_matches_only_the_selected_protocol`
foi adicionado, mas não pôde executar: a biblioteca de testes do cliente
falha antes disso em 24 erros anteriores, incluindo o arquivo ausente
`mod_registry/tests.rs`, uso de `tempfile` sem dependência e fixtures de mods
que referenciam campos e métodos de terreno/placas ausentes. O check da
biblioteca normal e do cliente Android passou; isso não equivale a esse teste.

A tentativa de `tools/pr_ready.py --base origin/codex/nivora-home-modrinth
--quick` também não ficou verde: descoberta Python conflita entre os dois
`test_basic_feature_parity`, a formatação global já diverge na base e o Clippy
encontra `as_chunks` em `horizon-core/src/prepared_files.rs`, disponível desde
Rust 1.88, enquanto esse crate declara mínimo 1.85. A etapa posterior de
testes gerais foi interrompida depois desses bloqueios. Não há alegação de
aprovação da suíte completa do workspace.

## Limites

Esse teste valida o transporte e as interações enumeradas. Não valida a
renderização, um APK em um celular, desempenho, servidores públicos,
autenticação online, combate completo, geração de mundos nativa, compatibilidade
de saves de jogadores nem as demais regras de singleplayer da 1.7. Um resultado
positivo não significa que o motor histórico inteiro esteja concluído.
