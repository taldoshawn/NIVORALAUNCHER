# Jogar, perfis e mapas no Android

## APK e primeira abertura

Baixe a [release assinada](https://github.com/taldoshawn/pojav/releases)
adequada à arquitetura do aparelho. ARM64 é a opção usual para celulares atuais.
Selecione a versão no card principal. Se os recursos ainda não estiverem
instalados, toque em **Instalar**, aguarde sua verificação e toque em **Jogar**. O teste em instalação limpa baixou cerca de 504 MiB.
A tela de boas-vindas do jogo tem um botão **Continue**. Para jogar
online, abra **Multiplayer**. Mundos locais em **Singleplayer** usam o motor
26.3 e ficam indisponíveis quando o protocolo 1.8.9 estiver selecionado.

A criação e entrada em mundo plano foram observadas no emulador. Outros tipos
de mundo, importações arbitrárias e desempenho em celulares não estão certificados.
Veja [as verificações realizadas](android-validation.md).

## Perfis locais e contas autenticadas

O botão da conta fica no canto superior direito do launcher. Escolha o perfil
local para jogar sozinho, em LAN ou em servidores que aceitem modo offline.
Use um nome de 3 a 16 caracteres: letras ASCII, números ou `_`.

O seletor permite salvar vários nomes e alternar entre eles sem apagar as
contas Microsoft. Um perfil local não é uma conta Microsoft nem autentica em servidores
que exigem uma conta com licença. Selecionar um perfil local também não registra
automaticamente uma conta em servidores que tenham cadastro próprio.

O [Hypixel exige uma conta de Minecraft Java](https://support.hypixel.net/hc/en-us/articles/360019495360-How-to-Join-the-Hypixel-Server).
O [Mush recomenda a 1.8.9](https://forum.mush.com.br/topic/327396/ajuda).
Selecione essa versão para usar o backend de protocolo 47 e consulte
[as instruções de validação e diagnóstico do Mush](android-multiplayer.md). Nem uma listagem de versões aceitas pelo
servidor nem uma resposta de ping comprovam compatibilidade de gameplay.

## Mapas existentes

A base Android oferece importação de pasta e de ZIP de mundos Java. Nas
configurações, abra o gerenciador de arquivos e a pasta **Worlds** do perfil
escolhido; use **Import** para copiar o mundo. Um ZIP de mundo Java contém
`level.dat` e normalmente uma pasta `region`. A importação usa uma nova pasta,
para não sobrescrever um mundo com o mesmo nome.

Importe cópias e mantenha o original: abrir um mundo antigo no motor atual pode
atualizar seus dados. O motor possui conversores de saves antigos, inclusive do
formato da 1.8, mas isso não significa executar o cliente 1.8.9. Mundos Bedrock,
mods Java, datapacks e recursos específicos de outras versões precisam de
compatibilidade própria. Não há promessa de abrir qualquer mapa sem diferenças.

## Versões

- **26.3 / protocolo 777:** motor atual; mundo plano validado em emulador.
- **1.8–1.8.9 / protocolo 47:** backend integrado ao jogo, com entrada no mundo,
  movimento, colocar/quebrar blocos e um ataque confirmados contra um servidor
  Cuberite isolado em emulador. Compatibilidade completa com servidores públicos
  e seus anti-cheats ainda não está comprovada; não inclui singleplayer 1.8.9.
- **Demais versões:** não implementadas como backends selecionáveis.

Os perfis numerados do launcher separam pastas, configurações e mundos. Eles
não trocam a versão do motor. A seleção 1.8.9 troca o protocolo multiplayer;
não executa um cliente Java nem converte mods Forge/OptiFine para Rust.
