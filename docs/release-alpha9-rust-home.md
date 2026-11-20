# NIVORA Launcher 0.1.0-alpha.9 — Interface Rust unificada

Esta pré-release experimental remove o caminho antigo do launcher Android em Kotlin.
Ao abrir o aplicativo, a única tela inicial é o launcher nativo em Rust, em modo paisagem,
com navegação lateral e banner Minecraft, conforme o layout oficial.

## Mudanças
- Visual da interface Rust aprimorado: navegação violeta, cards, botões e contraste da arte
- Evita reintroduzir a tela vertical com oito botões por meio de teste de manifesto Android
- Mantém os recursos existentes: conta, skins, mods, packs, shaders, HUD, configurações
- Importação de controles PojavLauncher, teclado e mouse Android, editor de HUD e correções anteriores
- Corrige reutilização indevida de sombras no Android e compatibilidade GLSL (mc_chunkFade)
- 22 shaders do catálogo verificados na etapa de tradução/teste; a aparência final depende do dispositivo

## Validação
Artefato universal para ARM64, ARMv7, x86 e x86_64, Android 9+.
A publicação é bloqueada caso a compilação, auditoria, hash SHA-256, ZIP, alinhamento
ou certificado oficial de assinatura falhem.

**Limitações**: instalação em aparelho físico, desempenho/fps de longo prazo, sombras em jogo real,
entrada USB/Bluetooth e paridade completa 1.8.9 com servidores ainda não foram comprovados.
O motor e os recursos são 26.3; 1.8.9 é uma camada multiplayer experimental de protocolo 47.
Não há compatibilidade geral com Forge/OptiFine Java.

Para atualizar a instalação oficial por cima, o Android precisa da mesma assinatura e de
um versionCode maior. O aplicativo de testes com ID separado pode continuar instalado em paralelo;
faça backup antes de remover qualquer instalação antiga.
