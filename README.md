# NIVORA LAUNCHER para Android

Cliente nativo em Rust baseado no [Horizon Android](https://github.com/GoyDevv/Horizon-Android), com renderização Vulkan/OpenGL ES e multiplayer Java.
O motor do jogo executa em Rust, sem uma JVM executando Minecraft no celular. A integração com o Android mantém uma pequena ponte Java da Activity; ferramentas Java também participam da compilação e preparação de recursos.

## APK

Baixe os APKs assinados na [release experimental](https://github.com/taldoshawn/NIVORALAUNCHER/releases/tag/v0.1.0-alpha.3). Os builds intermediários ficam em **Actions → Android APK → Artifacts**; eles usam outro certificado e não substituem a release por atualização direta.
ARM64 é a arquitetura dos celulares atuais. As variantes ARMv7 e x86 de 32 bits são experimentais.

- ARM64 / x86_64: Android 9 (API 28) ou superior.
- ARMv7 / x86: Android 8.1 (API 27) ou superior.
- GPU com OpenGL ES 3 ou Vulkan compatível; memória e drivers também limitam o funcionamento.
- Na primeira abertura, toque em **Instalar** para obter os recursos; depois, use **Jogar**. A seta ao lado do botão permite selecionar **1.8.9** para multiplayer de protocolo 47 ou **26.3**. Os mundos locais permanecem na versão do motor moderno.
- Servidores autenticados precisam de uma conta que possua Minecraft Java.

## Compatibilidade atual

O motor existente tem como alvo **26.3, protocolo 777**. A camada de rede
**1.8–1.8.9, protocolo 47**, está integrada ao jogo e é experimental.
O [teste do APK nº 21](https://github.com/taldoshawn/NIVORALAUNCHER/actions/runs/43623654873)
instalou a variante x86_64 no emulador Android 15, abriu as seis páginas principais,
importou um resource pack da pasta externa e confirmou movimento, quebra,
colocação e um ataque contra outro cliente de teste em um servidor Cuberite local.
Isso não comprova combate completo, servidores públicos, desempenho em um celular
físico nem compatibilidade universal. Forge, OptiFine e singleplayer 1.8.9 não
estão implementados.

A release inclui [documentação e exemplos](https://github.com/taldoshawn/NIVORALAUNCHER/releases/download/v0.1.0-alpha.3/Pojav-Rust-Documentacao-e-Exemplos.zip)
e [capturas reais do APK](https://github.com/taldoshawn/NIVORALAUNCHER/releases/download/v0.1.0-alpha.3/Pojav-Rust-Capturas-do-APK.zip).
Consulte as [pastas de conteúdo](docs/android-content-folders.md),
a [documentação de desenvolvimento](docs/pojav-development.md),
o [estado do protocolo legado](docs/legacy-protocol.md) e a
[compilação deste fork](docs/pojav-build.md).

## Origem e licenças

Snapshot de GoyDevv/Horizon-Android no commit `ae4c85dc5b31da2078ee1e1009a53ced7d7ed1ba`, com alterações locais. O histórico Git original não está incluído neste snapshot.

Créditos: Ludovic Marie, colaboradores do Horizon e mantenedores do port Android. Licença [MIT](LICENSE), com [avisos de terceiros](THIRD_PARTY_NOTICES.md). [README original](docs/upstream-README.md).
Este fork não é o PojavLauncher oficial e não é afiliado à Mojang ou Microsoft. JARs, mundos, contas, chaves e recursos originais do Minecraft não são enviados para este repositório.
