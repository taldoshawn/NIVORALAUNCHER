# Minecraft 1.7: transporte nativo em Rust

O cliente tem um caminho próprio para os protocolos 4 e 5 da família 1.7.
O estado atual é **multiplayer e criativo plano restrito experimentais**. Login, chunks, inventário,
movimento e interações básicas foram exercitados contra servidores vanilla
1.7.2 e 1.7.10. Isso não completa o motor histórico nem o singleplayer geral.

## Selecionar

No launcher Android, selecione **Versão → 1.7.2** ou **1.7.10** e entre em **Multiplayer**.
A escolha é preservada no perfil do launcher. Ela inicia o transporte de
protocolo 4 ou 5 e identifica a sessão como experimental. A consulta e a indicação
de compatibilidade na lista de servidores acompanham o protocolo selecionado.

No cliente de desktop, depois da preparação normal dos recursos do projeto:

```sh
horizon-client --protocol-5 --server 127.0.0.1:25565 --name Jogador
horizon-client --protocol-4 --server 127.0.0.1:25565 --name Jogador
```

| Seleção | Família do protocolo | Referência testada |
|---|---|---|
| `--protocol-4` | 1.7.2–1.7.5 | 1.7.2 |
| `--protocol-5` | 1.7.6–1.7.10 | 1.7.10 |

As versões intermediárias compartilham esses números de protocolo, mas não
receberam testes individuais nesta alteração. O seletor Android oferece as
referências 1.7.2 e 1.7.10; perfis anteriores da 1.7 continuam na 1.7.10.

O botão **Criativo plano** do menu principal abre o servidor nativo em memória
da versão selecionada. Cada perfil guarda mundos distintos em
`legacy-worlds/1.7.2/classic-flat` e `legacy-worlds/1.7.10/classic-flat`, relativos
à pasta de jogo. O motor usa arrays, altura, luz e NBT históricos próprios.
Não inicia o mundo 1.8.9 ou a simulação moderna sob o nome da 1.7.

Esse experimento oferece terreno plano e nove blocos fixos na barra, colocação,
quebra, colisão de cubos e persistência de posição, rotação e slot. O inventário
geral fica fechado nesse modo; os nove slots selecionam a paleta suportada.
O servidor continua responsável por validar alcance, colisão e alterações.
Mundos sem a marca do experimento, de outra versão ou com dados inválidos são
recusados antes de substituir seu conteúdo. Não há sobrevivência, criaturas,
receitas, redstone, geração normal ou paridade completa de movimento/iluminação.

O nome local funciona em servidores que aceitam perfis offline. Quando um
servidor exige autenticação, o transporte utiliza a conta já autenticada no
launcher e o serviço oficial de sessão. Uma falha não causa uma nova tentativa
offline. A autenticação online contra servidores públicos ainda precisa de
validação prática.

## Implementação

`crates/horizon-protocol/src/legacy17/` contém os codecs históricos:

- Handshake com a versão escolhida, login, UUID textual, arrays RSA com tamanho
  de dois bytes e ativação de AES após a resposta de criptografia. Não há
  negociação de compressão de pacotes ou fase de configuração moderna.
- Keepalive com inteiro de 32 bits, entrada e troca de dimensão, posição,
  rotação e campo separado de altura dos olhos (*stance*).
- Chunks individuais e em lote com zlib, máscaras de seções, extensão `Add`
  para IDs de 12 bits, luz e biomas. A luz do céu acompanha a dimensão.
- Slots históricos com NBT gzip e tamanho de dois bytes. As APIs explícitas
  `read_named_legacy` e `write_named_legacy` preservam listas de compounds
  históricos sem a normalização usada pelo formato moderno.
- Conversão de jogadores, propriedades de skin, nomes de entidades, movimento
  relativo, equipamento, janelas e inventário para eventos já entendidos pelo
  renderer; codificação de movimento, chat, ações, colocação, quebra, seleção
  de slot, cliques e configurações do cliente para o protocolo 1.7.

`crates/horizon-client/src/net/legacy17.rs` conecta esses codecs à sessão do
cliente. O modelo de eventos e de ações do caminho legado existente é
reutilizado internamente. Antes de enviar uma ação, o adaptador a recodifica
para os campos da 1.7. Os bytes do protocolo 47 não são enviados diretamente
ao servidor 1.7.

O caminho usa o renderer, recursos preparados e parte da predição local já
existentes. Portanto, não estabelece equivalência visual ou comportamental
com o cliente original 1.7. O servidor remoto permanece responsável pela
simulação autoritativa do mundo.

## Limites de entrada e proteção dos mundos

Os decoders recusam pacotes truncados, comprimentos inválidos, dados extras
nos formatos reconhecidos e fluxos zlib/gzip inválidos. Há limites para a
expansão descomprimida, para a expansão agregada de inventários/chunks
normalizados e para nomes de jogadores armazenados. Pacotes sem conversão
implementada não são encaminhados como se fossem de outra versão.

O menu oferece apenas a entrada explícita do criativo plano restrito. Os caminhos de
abertura de mundos gerais continuam recusando essa seleção, incluindo `--world`,
`--quickPlaySingleplayer`, benchmark e servidor dedicado. Assim, selecionar
1.7 não abre o mundo plano 1.8.9 nem o motor de mundos moderno sob outro rótulo.

## O que ainda falta

- Motor local completo da 1.7: registros próprios, regras de blocos e itens, receitas,
  criaturas, combate, redstone, geração de terreno, estruturas e persistência
  com validação histórica além do núcleo plano restrito.
- Cobertura completa de pacotes e apresentação: sons, partículas, mapas,
  placas, vínculos de entidades e certos metadados e tipos de menu continuam
  sem paridade comprovada. Servidores com plugins podem exigir extensões ainda
  não suportadas.
- Renderização e controles testados em aparelho Android, desempenho e sessões
  longas. A compilação ARM64 não substitui esses testes.
- Clientes e motores nativos das demais versões históricas. Os diagnósticos
  de status e os testes existentes de arquivos de região para 1.12.2/1.20.1
  não são implementações desses clientes ou motores.

## Testar

```sh
cargo test -p horizon-protocol --locked --lib --test legacy18 \
  --test legacy17 --test legacy17_chunks --test legacy17_bridge_review \
  --test legacy17_limits
cargo test -p horizon-world --locked --lib nbt::tests
cargo check -p horizon-client --bin horizon-client --no-default-features --locked
cargo run -p horizon-client --example legacy17_local_probe --no-default-features --locked -- .local/session17-new-run
python3 tools/test_legacy17_server.py --version 1.7.2
python3 tools/test_legacy17_server.py --version 1.7.10
```

O [relatório de validação](legacy17-validation.md) explica os valores
independentes, as ações confirmadas pelos servidores e os limites das
evidências. Os JARs oficiais são referências locais de desenvolvimento;
não integram o código publicado ou o runtime do launcher.

## Referência de formato

A revisão dos campos históricos consultou `Protocol17x.cpp`,
`Protocol18x.cpp` e `ChunkDataSerializer.cpp` do [Cuberite no commit
72aa50f710200832f97a0022dc70c5cd531b6aea](https://github.com/cuberite/cuberite/tree/72aa50f710200832f97a0022dc70c5cd531b6aea),
após verificar a licença Apache-2.0 e os cabeçalhos dos arquivos nessa revisão.
Os codecs Rust foram implementados no projeto; não foi incorporado código
C++ do Cuberite. Os servidores originais foram usados como referência de
comportamento somente nos testes locais.
