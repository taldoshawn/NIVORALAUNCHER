# Validação privada: HUD Pojav e periféricos

Esta alteração permanece na branch local `codex/private-hud-peripherals`. Não envolve website,
release, links de download, atualização remota de conteúdo nem publicação no GitHub.

## Arquitetura e alterações

O APK usa `NativeActivity`: a interface e o jogo são Rust; Java faz a ponte com serviços Android
(IME, seletor de documentos, permissões e ciclo de vida). `classes.dex` pequeno e uma biblioteca
nativa grande são esperados. O número de DEX não mede funcionalidades nem qualidade. Não foram
adicionadas dependências para aumentar o APK.

- `horizon-client-render/src/touch/pojav.rs`: importação GLFW v8, comandos especiais, expressões
  aritméticas limitadas e preservação das expressões no arquivo salvo. `px()` usa a tela lógica
  em dp; altura/densidade são informadas pelo Android, independentemente da resolução de renderização.
  Não executa JavaScript, comandos de shell nem código do JSON. Cache limitado evita analisar a
  expressão novamente a cada frame. As faixas SEC estreitas não passam pelo limite mínimo dos
  botões comuns. Nomes vazios dos botões importados são preservados, sem legenda automática.
  Metadados, gavetas e um analógico usam as estruturas existentes.
- `touch/{layout,custom,editor,property_input,drawer}.rs` e `touch_ui.rs`: dimensões independentes
  preservadas na edição/importação; expressões só se tornam posições editadas quando se altera
  geometria; clonagem resolve o tamanho no viewport atual sem modificar o original. Renomeação
  conserva a geometria dinâmica e usa o editor/IME existente. Botões novos continuam neutros.
- `mouse_state.rs`: os eixos relativos têm prioridade sobre saltos do cursor absoluto; movimentos
  não finitos são descartados. Não há conversão de mouse para toques arbitrários.
- `HorizonActivity.java`, `document_bridge.rs`, `android_runtime.rs`, `game.rs`: desconectar um
  periférico solicita liberação dos estados de entrada na thread do jogo. Não acessa `Game` pela
  thread Java. O ganho artificial de 2,6x do mouse físico foi removido; a sensibilidade do jogo é
  mantida. Captura/liberação existentes seguem o serviço Android; USB/Bluetooth usam a mesma API.
- `android/{chooser,launcher_pages}.rs` e `launcher_pages/reference.rs`: seletores mais discretos,
  limites de texto, traduções PT-BR e acesso explícito ao editor HUD no cabeçalho e nas configurações.
  A composição nativa, renderizadores e ações existentes são preservados.

O protocolo multiplayer, a física, as contas, os dados de usuário e o motor 26.3 não foram substituídos.
Esta alteração não implementa Forge nem compatibilidade binária geral com mods Java.

## Arquivo de referência

`crates/horizon-client/tests/fixtures/mush-v8.json` é o arquivo fornecido pelo proprietário:
44 controles, versão 8, `scaledAt=100`, sem gavetas/analógicos. Há diagonais com duas teclas e
múltiplas faixas SEC de aproximadamente 173,33 × 4,95 unidades lógicas. O arquivo associa o botão
chamado F5 à tecla GLFW F e o chamado Ctrl à tecla R. Essas associações são preservadas, não
corrigidas por inferência do nome.

A interoperabilidade usa nomes de campos, valores de enumeração e constantes numéricas do formato
PojavLauncher (https://github.com/PojavLauncherTeam/PojavLauncher, controles `v3_openjdk`). O parser
aritmético desta alteração é implementação própria; não incorpora o avaliador ou código LGPL do
PojavLauncher.

Comandos especiais: -1 teclado, -2 visibilidade dos controles, -3 ataque, -4 interação, -5 mouse
virtual, -6 botão central, -7 scroll para cima, -8 scroll para baixo, -9 menu do launcher.

## Limitações explícitas

- O motor possui um analógico de movimento. Mais de um analógico e `forwardLock=true` retornam erro
  explícito; não são silenciosamente convertidos em teclas sem relação.
- O teste principal cobre versão 8/escala 100. Layouts v9 com códigos Android antigos têm detecção
  compatível com os arquivos produzidos anteriormente pelo Nivora; arquivos ambíguos só com códigos
  baixos precisam de validação adicional. Escalas diferentes de 100 também exigem comparação no aparelho.
- A importação preserva propriedades adicionais no JSON de origem. Isso não significa suporte de
  execução para todo recurso adicional, como imagens de fundo externas de terceiros.
- Alterar posição/tamanho materializa a geometria do controle editado; controles não editados mantêm
  suas expressões responsivas. Não é prometida equivalência de todo layout Pojav em todo aparelho.
- Não há dispositivo Android conectado nem teste de gameplay/periféricos USB/BT neste ambiente.
  Testes de estado de entrada não substituem esses testes.
- O acabamento foi alterado em código; capturas reais das telas novas e validação visual no celular
  permanecem pendentes. Não foram gerados novos assets nesta alteração.

## APK de desenvolvimento

Pacote separado: `dev.taldoshawn.nivora.private`; ARM64; mínimo Android 9/API 28; alvo API 35.
Nome de versão local: `0.1.0-private.hud.1`, código 1. A instalação é paralela à versão pública.
Usa certificado de desenvolvimento local, não a chave permanente de distribuição. Não é uma
atualização instalável por cima do pacote público. Credenciais e keystores não são versionados.
O fluxo de aviso de atualização pública fica desativado neste build privado.

O Rust é compilado pelo script existente `tools/android_build.py`, perfil fast ajustado para
compilação local. O argumento `--public` desse script seleciona somente tabelas geradas permitidas
para empacotamento; não publica arquivos. Para esta compilação usa-se `NIVORA_PRIVATE_TEST=1`,
`--rust-only`, `--abi arm64-v8a` e o identificador privado.

Por limitação de disco, a montagem local usa diretamente as ferramentas oficiais SDK 35
(aapt2, compilador Java, D8, zipalign e apksigner), sem baixar Gradle/AGP adicionais. Usa o manifesto,
recursos, fontes Java, assets permitidos e biblioteca nativa do projeto. O script temporário de
empacotamento fica em `.local/private-test/package.py`; não muda o pipeline público existente.
O certificado local permite repetir testes privados, mas não deve ser usado para distribuição.

## Verificação no celular antes de qualquer distribuição

1. Instalar ao lado da versão pública; abrir e confirmar nome de teste privado.
2. Adicionar conta e baixar os dados oficiais; selecionar 1.8.9 e servidor de teste.
3. Importar o arquivo mush; verificar 44 controles, SEC, diagonais e a ação de menu -9.
4. Renomear com Gboard (digitar, apagar, acentos), concluir, sair e reabrir.
5. Ajustar uma largura, depois uma altura; confirmar que outros controles não mudam.
6. Clonar, mover, salvar, exportar e importar novamente; reiniciar e conferir persistência.
7. Conectar teclado/mouse USB e Bluetooth: WASD, pular segurado, Shift, Ctrl, hotbar, F1–F5,
   chat, inventário, teclas simultâneas, cliques, scroll, captura/liberação do cursor.
8. Desconectar enquanto uma tecla/botão estiver pressionado e verificar liberação.
9. Alternar touchscreen/periféricos, rotacionar e testar diferentes resoluções/renderizadores.
10. Validar abertura de mundos 26.3 e multiplayer 1.8.9. Registrar logs e modelo do aparelho.

Resultados automatizados e integridade do APK são registrados separadamente após a compilação.

## Resultados automatizados desta alteração

- `cargo test -p horizon-client --no-default-features --test hud_editor --test pojav_import --locked`:
  **12 passaram** (5 editor/IME; 7 Pojav). Inclui o fluxo real de importação/ativação em uma pasta
  temporária, reabertura do perfil, 44 controles, diagonais, exportação, rótulos vazios, dimensões
  estreitas, clonagem independente e rejeição de expressões inválidas/código arbitrário.
- `rustc --test third_party/winit/src/platform_impl/android/mouse_state.rs`: **8 passaram**.
- Fontes Java compiladas e convertidas pelo D8; recursos compilados/linkados pelo aapt2 do SDK 35.
- Compilação Rust ARM64 concluída; artefato final é recompilado depois do commit local para registrar
  sua identificação correta. Build de desenvolvimento `fast`, opt-level 1, sem LTO ou debug info Rust;
  não foi feita comparação de desempenho com o APK público otimizado para release.
- Auditoria de payload: somente `debuggable`, esperado neste pacote privado. O verificador de release
  portanto **não aprova este APK para distribuição**. Nenhum outro achado nessa checagem limitada.
  Não se trata de certificação exaustiva de segurança.
- CRC do ZIP, assinatura apksigner, zipalign e alinhamento ELF de 16 KiB verificados no primeiro APK;
  são repetidos no artefato final. SHA-256, estrutura do pacote e logs finais ficam junto ao APK em
  `.local/private-test/`. Esse diretório é ignorado pelo Git, inclusive certificado e ferramentas.
- `tools/check_public_tree.py` e verificação de whitespace passaram. Nenhum push/deploy/release executado.

Não foram testados instalação física, gameplay, Gboard real, periféricos físicos, Androids diferentes
nem FPS/frame time em aparelho. Não há afirmação de ganho de FPS ou de conclusão desses testes.
